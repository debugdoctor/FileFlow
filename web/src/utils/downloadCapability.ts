export const isMobileDownloadClient = (
  userAgent: string,
  maxTouchPoints = 0,
  userAgentDataMobile?: boolean,
) => {
  if (userAgentDataMobile === true) return true;

  const mobileUserAgent = /Android|webOS|iPhone|iPad|iPod|BlackBerry|IEMobile|Opera Mini|Mobile/i.test(userAgent);
  const iPadUsingDesktopSite = /Macintosh/i.test(userAgent) && maxTouchPoints > 1;
  return mobileUserAgent || iPadUsingDesktopSite;
};
