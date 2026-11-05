use std::{collections::{hash_map::RandomState, HashMap, VecDeque}, hash::BuildHasher, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};

use axum::{extract::{Path, Query}, http::{header, HeaderMap, StatusCode}, response::{Html, IntoResponse}, Json};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Mutex;

use super::static_files::StaticFiles;
use crate::utils::nanoid;

const ROOM_TTL: Duration = Duration::from_secs(3600);
const MAX_ROOMS: usize = 128;
const MAX_TEXT_BYTES: usize = 64 * 1024;
const MAX_HISTORY_BYTES: usize = 256 * 1024;
const MAX_MESSAGES: usize = 100;
const MAX_PARTICIPANTS: usize = 32;
const ONLINE_WINDOW: Duration = Duration::from_secs(10);

lazy_static! {
    static ref ROOMS: Mutex<TextRooms> = Mutex::new(TextRooms::default());
}

type Failure = (StatusCode, &'static str);

#[derive(Clone, Serialize)]
struct Message {
    seq: u64,
    id: String,
    author: u64,
    author_name: String,
    text: String,
    created_at: u64,
}

struct Participant {
    id: u64,
    token: String,
    name: String,
    seen: Instant,
}

fn anonymous_name(token: &str, id: u64) -> String {
    const MOODS: [&str; 16] = ["漫游", "轻快", "安静", "好奇", "悠闲", "晴天", "追风", "听雨", "星光", "微笑", "暖阳", "清晨", "月光", "云端", "森林", "海边"];
    const ANIMALS: [&str; 16] = ["海豚", "小鹿", "狐狸", "白鲸", "熊猫", "松鼠", "企鹅", "海獭", "猫咪", "考拉", "小熊", "飞鸟", "水豚", "小兔", "柴犬", "雪豹"];
    let random = RandomState::new().hash_one((token, id));
    format!("{}{}", MOODS[(random & 15) as usize], ANIMALS[((random >> 4) & 15) as usize])
}

struct Room {
    creator: String,
    participants: Vec<Participant>,
    next_participant_id: u64,
    expires: Instant,
    messages: VecDeque<Message>,
    bytes: usize,
    seq: u64,
}

impl Room {
    fn participant(&self, token: &str) -> Result<u64, Failure> {
        self.participants.iter().find(|member| member.token == token).map(|member| member.id)
            .ok_or((StatusCode::FORBIDDEN, "会话凭据无效，请重新加入"))
    }

    fn touch(&mut self, id: u64) {
        let now = Instant::now();
        self.expires = now + ROOM_TTL;
        if let Some(member) = self.participants.iter_mut().find(|member| member.id == id) { member.seen = now; }
    }
}

#[derive(Default)]
struct TextRooms {
    rooms: HashMap<String, Room>,
}

fn valid_hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl TextRooms {
    fn create(&mut self, token: &str) -> Result<String, Failure> {
        self.create_with_generator(token, nanoid::generate)
    }

    fn create_with_generator(&mut self, token: &str, mut generate: impl FnMut() -> String) -> Result<String, Failure> {
        if !valid_hex(token, 64) {
            return Err((StatusCode::BAD_REQUEST, "无效的会话凭据"));
        }
        let now = Instant::now();
        self.rooms.retain(|_, room| room.expires > now);
        // Retrying a create after a lost response must recover the original room.
        if let Some((id, _)) = self.rooms.iter().find(|(_, room)| room.creator == token) {
            return Ok(id.clone());
        }
        if self.rooms.len() >= MAX_ROOMS {
            return Err((StatusCode::SERVICE_UNAVAILABLE, "当前会话较多，请稍后再试"));
        }
        let id = (0..16).map(|_| generate()).find(|id| !self.rooms.contains_key(id))
            .ok_or((StatusCode::SERVICE_UNAVAILABLE, "暂时无法分配房间码，请重试"))?;
        self.rooms.insert(id.clone(), Room {
            creator: token.to_string(),
            participants: vec![Participant { id: 1, token: token.into(), name: anonymous_name(token, 1), seen: now }],
            next_participant_id: 2,
            expires: now + ROOM_TTL, messages: VecDeque::new(), bytes: 0, seq: 0,
        });
        Ok(id)
    }

    fn room(&mut self, id: &str) -> Result<&mut Room, Failure> {
        if self.rooms.get(id).is_some_and(|room| room.expires <= Instant::now()) {
            self.rooms.remove(id);
        }
        self.rooms.get_mut(id).ok_or((StatusCode::NOT_FOUND, "会话已结束或过期，请创建新会话"))
    }

    fn join(&mut self, id: &str, token: &str) -> Result<(), Failure> {
        if !valid_hex(token, 64) { return Err((StatusCode::BAD_REQUEST, "无效的会话凭据")); }
        let room = self.room(id)?;
        // The caller keeps its credential before requesting, so a lost response can be retried.
        if let Ok(member) = room.participant(token) { room.touch(member); return Ok(()); }
        if room.participants.len() >= MAX_PARTICIPANTS { return Err((StatusCode::CONFLICT, "房间已满（最多 32 人），请等待成员退出")); }
        let id = room.next_participant_id;
        let mut name = anonymous_name(token, id);
        // Keep names distinct from both current members and retained messages after a member leaves.
        if room.participants.iter().any(|member| member.name == name) || room.messages.iter().any(|message| message.author_name == name) {
            name = format!("{name}·{id}");
        }
        room.participants.push(Participant { id, token: token.into(), name, seen: Instant::now() });
        room.next_participant_id += 1;
        room.touch(id);
        Ok(())
    }

    fn read(&mut self, id: &str, token: &str, since: u64) -> Result<serde_json::Value, Failure> {
        let room = self.room(id)?;
        let self_id = room.participant(token)?;
        room.touch(self_id);
        let member = room.participants.iter().find(|member| member.id == self_id).unwrap();
        Ok(json!({
            "role": if self_id == 1 { "creator" } else { "guest" },
            "self_id": self_id,
            "self_name": member.name,
            "participant_count": room.participants.len(),
            "online_count": room.participants.iter().filter(|member| member.seen.elapsed() < ONLINE_WINDOW).count(),
            "max_participants": MAX_PARTICIPANTS,
            "messages": room.messages.iter().filter(|message| message.seq > since).collect::<Vec<_>>(),
            "latest_seq": room.seq,
            "oldest_seq": room.messages.front().map_or(room.seq + 1, |message| message.seq),
        }))
    }

    fn send(&mut self, id: &str, token: &str, payload: SendPayload) -> Result<Message, Failure> {
        if !valid_hex(&payload.id, 32) || payload.text.trim().is_empty() || payload.text.len() > MAX_TEXT_BYTES {
            return Err((StatusCode::BAD_REQUEST, "文本不能为空，且每条不能超过 64 KiB"));
        }
        let room = self.room(id)?;
        let author = room.participant(token)?;
        room.touch(author);
        if let Some(message) = room.messages.iter().find(|message| message.author == author && message.id == payload.id) {
            return if message.text == payload.text { Ok(message.clone()) }
                else { Err((StatusCode::CONFLICT, "请勿使用同一消息编号发送不同文本")) };
        }
        room.seq += 1;
        let author_name = room.participants.iter().find(|member| member.id == author).unwrap().name.clone();
        let message = Message {
            seq: room.seq, id: payload.id, author, author_name, text: payload.text,
            created_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64,
        };
        room.bytes += message.text.len();
        room.messages.push_back(message.clone());
        while room.bytes > MAX_HISTORY_BYTES || room.messages.len() > MAX_MESSAGES {
            if let Some(old) = room.messages.pop_front() { room.bytes -= old.text.len(); }
        }
        Ok(message)
    }

    fn close(&mut self, id: &str, token: &str) -> Result<(), Failure> {
        if self.room(id)?.participant(token)? != 1 { return Err((StatusCode::FORBIDDEN, "只有房主可以结束房间")); }
        self.rooms.remove(id);
        Ok(())
    }

    fn leave(&mut self, id: &str, token: &str) -> Result<(), Failure> {
        let room = self.room(id)?;
        if token == room.creator { return Err((StatusCode::FORBIDDEN, "房主请使用结束房间")); }
        // Idempotent so leaving can be retried after a lost response.
        room.participants.retain(|member| member.token != token);
        Ok(())
    }
}

#[derive(Deserialize)]
pub struct CreatePayload { token: String }
#[derive(Deserialize)]
pub struct JoinPayload { token: String }
#[derive(Deserialize)]
pub struct SendPayload { id: String, text: String }
#[derive(Deserialize)]
pub struct ReadQuery { since: Option<u64> }

fn token(headers: &HeaderMap) -> Result<&str, Failure> {
    headers.get(header::AUTHORIZATION).and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer ")).filter(|value| valid_hex(value, 64))
        .ok_or((StatusCode::UNAUTHORIZED, "缺少会话凭据"))
}

fn respond<T: Serialize>(result: Result<T, Failure>) -> axum::response::Response {
    let mut response = match result {
        Ok(data) => Json(json!({"success": true, "data": data})).into_response(),
        Err((status, message)) => (status, Json(json!({"success": false, "message": message}))).into_response(),
    };
    response.headers_mut().insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}

pub async fn page() -> axum::response::Response {
    match StaticFiles::get("text/index.html") {
        Some(content) => {
            let mut response = Html(String::from_utf8_lossy(&content.data).into_owned()).into_response();
            response.headers_mut().insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
            response
        },
        None => (StatusCode::NOT_FOUND, "Page not found").into_response(),
    }
}

pub async fn create(Json(payload): Json<CreatePayload>) -> axum::response::Response {
    respond(ROOMS.lock().await.create(&payload.token).map(|id| json!({"room_id": id})))
}

pub async fn join(Path(id): Path<String>, Json(payload): Json<JoinPayload>) -> axum::response::Response {
    respond(ROOMS.lock().await.join(&id, &payload.token))
}

pub async fn read(Path(id): Path<String>, Query(query): Query<ReadQuery>, headers: HeaderMap) -> axum::response::Response {
    let credential = match token(&headers) { Ok(value) => value, Err(error) => return respond::<()>(Err(error)) };
    respond(ROOMS.lock().await.read(&id, credential, query.since.unwrap_or(0)))
}

pub async fn send(Path(id): Path<String>, headers: HeaderMap, Json(payload): Json<SendPayload>) -> axum::response::Response {
    let credential = match token(&headers) { Ok(value) => value, Err(error) => return respond::<()>(Err(error)) };
    respond(ROOMS.lock().await.send(&id, credential, payload))
}

pub async fn close(Path(id): Path<String>, headers: HeaderMap) -> axum::response::Response {
    let credential = match token(&headers) { Ok(value) => value, Err(error) => return respond::<()>(Err(error)) };
    respond(ROOMS.lock().await.close(&id, credential))
}

pub async fn leave(Path(id): Path<String>, headers: HeaderMap) -> axum::response::Response {
    let credential = match token(&headers) { Ok(value) => value, Err(error) => return respond::<()>(Err(error)) };
    respond(ROOMS.lock().await.leave(&id, credential))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn creator() -> String { "a".repeat(64) }
    fn guest() -> String { "b".repeat(64) }
    fn payload(id: usize, text: &str) -> SendPayload { SendPayload { id: format!("{id:032x}"), text: text.into() } }

    #[tokio::test]
    async fn page_disables_browser_caching() {
        assert_eq!(page().await.headers()[header::CACHE_CONTROL], "no-store");
    }

    #[test]
    fn bidirectional_history_and_retry_are_ordered_and_isolated() {
        let mut store = TextRooms::default();
        let id = store.create(&creator()).unwrap();
        assert_eq!(store.create(&creator()).unwrap(), id);
        assert_eq!(id.len(), 5);
        assert!(id.bytes().all(|byte| byte.is_ascii_digit() || byte.is_ascii_lowercase()));
        store.send(&id, &creator(), payload(1, "第一条\n<script>alert(1)</script>")).unwrap();
        store.join(&id, &guest()).unwrap();
        store.join(&id, &guest()).unwrap();
        let reply = store.send(&id, &guest(), payload(1, "收到 😀")).unwrap();
        assert_eq!(reply.seq, 2);
        assert_eq!(store.send(&id, &guest(), payload(1, "收到 😀")).unwrap().seq, 2);
        assert!(store.send(&id, &guest(), payload(1, "其他内容")).is_err());
        let creator_view = store.read(&id, &creator(), 1).unwrap();
        assert_eq!(creator_view["messages"].as_array().unwrap().len(), 1);
        assert_eq!(creator_view["messages"][0]["author"], 2);
        assert_eq!(store.read(&id, &guest(), 0).unwrap()["messages"][0]["author"], 1);
        assert_eq!(store.read(&id, &"c".repeat(64), 0).unwrap_err().0, StatusCode::FORBIDDEN);
        assert!(store.close(&id, &"c".repeat(64)).is_err());
        assert_eq!(store.close(&id, &guest()).unwrap_err().0, StatusCode::FORBIDDEN);
        store.close(&id, &creator()).unwrap();
        assert_eq!(store.read(&id, &creator(), 0).unwrap_err().0, StatusCode::NOT_FOUND);
    }

    #[test]
    fn limits_and_expiry_bound_memory() {
        let mut store = TextRooms::default();
        let id = store.create(&creator()).unwrap();
        assert!(store.send(&id, &creator(), payload(0, " \n ")).is_err());
        assert!(store.send(&id, &creator(), payload(0, &"中".repeat(MAX_TEXT_BYTES / 3 + 1))).is_err());
        for index in 0..110 { store.send(&id, &creator(), payload(index, "hello")).unwrap(); }
        assert_eq!(store.rooms[&id].messages.len(), MAX_MESSAGES);
        assert_eq!(store.read(&id, &creator(), 0).unwrap()["oldest_seq"], 11);
        for index in 110..120 { store.send(&id, &creator(), payload(index, &"x".repeat(MAX_TEXT_BYTES))).unwrap(); }
        assert_eq!(store.rooms[&id].bytes, MAX_HISTORY_BYTES);
        store.rooms.get_mut(&id).unwrap().expires = Instant::now() - Duration::from_secs(1);
        assert_eq!(store.read(&id, &creator(), 0).unwrap_err().0, StatusCode::NOT_FOUND);
        assert!(!store.rooms.contains_key(&id));
        for index in 0..MAX_ROOMS { store.create(&format!("{index:064x}")).unwrap(); }
        assert_eq!(store.create(&creator()).unwrap_err().0, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn short_code_collisions_retry_without_overwriting_an_existing_room() {
        let mut store = TextRooms::default();
        let id = store.create_with_generator(&creator(), || "abc12".into()).unwrap();
        let mut candidates = ["abc12", "xyz89"].into_iter();
        let second = store.create_with_generator(&guest(), || candidates.next().unwrap().into()).unwrap();
        assert_eq!(second, "xyz89");
        assert_eq!(store.rooms[&id].creator, creator());
        assert_eq!(store.create(&guest()).unwrap(), second);
        assert_eq!(store.create_with_generator(&"c".repeat(64), || "abc12".into()).unwrap_err().0, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn group_capacity_anonymous_identity_presence_and_leaving() {
        let mut store = TextRooms::default();
        let id = store.create(&creator()).unwrap();
        for index in 1..MAX_PARTICIPANTS { store.join(&id, &format!("{index:064x}")).unwrap(); }
        let snapshot = store.read(&id, &creator(), 0).unwrap();
        assert_eq!(snapshot["participant_count"], 32);
        assert_eq!(snapshot["online_count"], 32);
        assert_eq!(snapshot["max_participants"], 32);
        assert!(snapshot.get("token").is_none());
        let names = store.rooms[&id].participants.iter().map(|member| &member.name).collect::<std::collections::HashSet<_>>();
        assert_eq!(names.len(), 32);
        let first = format!("{:064x}", 1);
        let overflow = format!("{:064x}", 32);
        assert_eq!(store.join(&id, &overflow).unwrap_err().0, StatusCode::CONFLICT);
        let before_retry = store.read(&id, &first, 0).unwrap();
        store.join(&id, &first).unwrap(); // Existing credentials reconnect even in a full room.
        let after_retry = store.read(&id, &first, 0).unwrap();
        assert_eq!(before_retry["self_id"], after_retry["self_id"]);
        assert_eq!(before_retry["self_name"], after_retry["self_name"]);
        let first_message = store.send(&id, &first, payload(1, "first")).unwrap();
        let second_message = store.send(&id, &format!("{:064x}", 2), payload(1, "second")).unwrap();
        assert_ne!(first_message.author, second_message.author);
        assert_ne!(first_message.author_name, second_message.author_name);
        assert_eq!(first_message.author_name, before_retry["self_name"].as_str().unwrap());
        assert_eq!(store.read(&id, &creator(), 0).unwrap()["messages"].as_array().unwrap().len(), 2);
        store.rooms.get_mut(&id).unwrap().participants[1].seen = Instant::now() - Duration::from_secs(11);
        assert_eq!(store.read(&id, &creator(), 0).unwrap()["online_count"], 31);
        store.leave(&id, &first).unwrap();
        store.leave(&id, &first).unwrap();
        assert_eq!(store.read(&id, &first, 0).unwrap_err().0, StatusCode::FORBIDDEN);
        assert!(store.send(&id, &first, payload(2, "removed")).is_err());
        store.join(&id, &overflow).unwrap();
        assert_eq!(store.read(&id, &overflow, 0).unwrap()["self_id"], 33);
        assert_eq!(store.read(&id, &creator(), 0).unwrap()["participant_count"], 32);
        assert!(store.leave(&id, &creator()).is_err());
        assert!(store.close(&id, &overflow).is_err());
        store.close(&id, &creator()).unwrap();
        assert!(store.read(&id, &overflow, 0).is_err());
    }
}
