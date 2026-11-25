// The access token lives only in this module variable: never in
// localStorage or sessionStorage, so a page reload drops it and the app
// gets a new one from the httpOnly refresh cookie.
let accessToken: string | null = null;

export function getAccessToken(): string | null {
  return accessToken;
}

export function setAccessToken(token: string | null): void {
  accessToken = token;
}
