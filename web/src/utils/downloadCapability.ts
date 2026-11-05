export const isMobileDownloadClient = (
  userAgent: string,
  maxTouchPoints = 0,
  userAgentDataMobile?: boolean,
  coarsePointer = false,
) => {
  if (userAgentDataMobile === true || coarsePointer) return true;

  const mobileUserAgent = /Android|HarmonyOS|HUAWEI|HONOR|HuaweiBrowser|webOS|iPhone|iPad|iPod|BlackBerry|IEMobile|Opera Mini|Mobile/i.test(userAgent);
  const iPadUsingDesktopSite = /Macintosh/i.test(userAgent) && maxTouchPoints > 1;
  return mobileUserAgent || iPadUsingDesktopSite;
};
