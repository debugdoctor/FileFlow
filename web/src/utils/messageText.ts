export interface MessagePart { text: string; href?: string; host?: string }

// Render these parts through Vue text nodes, never as HTML. No remote previews are fetched.
export function splitMessageText(text: string): MessagePart[] {
  const parts: MessagePart[] = [];
  let cursor = 0;
  for (const match of text.matchAll(/\bhttps?:\/\/[^\s<>"'`，。！？；、（）【】「」]+/giu)) {
    let candidate = match[0];
    const brackets: Record<string, string> = { ')': '(', ']': '[', '}': '{' };
    const counts: Record<string, number> = {};
    for (const char of candidate) {
      if ('()[]{}'.includes(char)) counts[char] = (counts[char] ?? 0) + 1;
    }
    // Leave sentence punctuation and unmatched closing brackets outside the anchor.
    let end = candidate.length;
    while (end) {
      const last = candidate[end - 1]!;
      const opening = brackets[last];
      if (/[.,!?;:]/.test(last)) end--;
      else if (opening && (counts[last] ?? 0) > (counts[opening] ?? 0)) { counts[last]!--; end--; }
      else break;
    }
    candidate = candidate.slice(0, end);
    try {
      const url = new URL(candidate);
      if (!['http:', 'https:'].includes(url.protocol) || !url.hostname || url.username || url.password) continue;
      if (match.index > cursor) parts.push({ text: text.slice(cursor, match.index) });
      parts.push({ text: candidate, href: url.href, host: url.host });
      cursor = match.index + candidate.length;
    } catch { /* Invalid URLs remain ordinary text. */ }
  }
  if (cursor < text.length) parts.push({ text: text.slice(cursor) });
  return parts;
}
