const tokenEndpoint = 'https://open.tiktokapis.com/v2/oauth/token/';
const authorizationFields = ['grant_type', 'client_key', 'code', 'code_verifier', 'redirect_uri'];
const refreshFields = ['grant_type', 'client_key', 'refresh_token'];
const providerErrors = {
  invalid_grant: 'TikTok rejected this grant. Reconnect the account.',
  invalid_client: 'TikTok rejected the app credentials.',
  invalid_request: 'TikTok rejected the token request.',
  unauthorized_client: 'TikTok has not authorized this client.',
  access_denied: 'TikTok denied authorization.',
};

function json(status, value, headers = {}) {
  return Response.json(value, {
    status,
    headers: { 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff', ...headers },
  });
}

function failure(status, error, message, headers) {
  return json(status, { error, error_description: message }, headers);
}

// Also bound chunked requests for which the client omitted Content-Length.
async function readBody(request) {
  const maxBytes = 16384;
  if (!request.body || Number(request.headers.get('Content-Length')) > maxBytes) return null;
  const reader = request.body.getReader();
  const decoder = new TextDecoder('utf-8', { fatal: true });
  let bytes = 0;
  let body = '';
  try {
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      bytes += value.byteLength;
      if (bytes > maxBytes) {
        await reader.cancel();
        return null;
      }
      body += decoder.decode(value, { stream: true });
    }
    return JSON.parse(body + decoder.decode());
  } catch {
    return null;
  } finally {
    reader.releaseLock();
  }
}

function validGrant(input, env) {
  if (!input || typeof input !== 'object' || input.client_key !== env.TIKTOK_CLIENT_KEY) return false;
  const isCode = input.grant_type === 'authorization_code';
  if (!isCode && input.grant_type !== 'refresh_token') return false;
  const fields = isCode ? authorizationFields : refreshFields;
  if (Object.keys(input).length !== fields.length ||
      !fields.every(key => Object.hasOwn(input, key) && typeof input[key] === 'string' && input[key].length > 0)) {
    return false;
  }
  return !isCode || (input.redirect_uri === env.TIKTOK_REDIRECT_URI &&
    /^[A-Za-z0-9._~-]{43,128}$/.test(input.code_verifier));
}

export async function handleRequest(request, env, fetchUpstream = fetch) {
  const url = new URL(request.url);
  if (url.protocol !== 'https:') return failure(400, 'invalid_request', 'HTTPS is required.');
  if (url.pathname !== '/v1/tiktok/token' || url.search) return failure(404, 'not_found', 'Unknown endpoint.');
  if (request.method !== 'POST') return failure(405, 'invalid_request', 'Use POST.', { Allow: 'POST' });
  if (request.headers.get('Content-Type')?.split(';')[0].trim().toLowerCase() !== 'application/json') {
    return failure(415, 'invalid_request', 'Use an application/json request body.');
  }
  if (!env.TIKTOK_CLIENT_KEY || !env.TIKTOK_CLIENT_SECRET || !env.TIKTOK_REDIRECT_URI) {
    return failure(503, 'server_error', 'Token broker configuration is incomplete.');
  }
  const input = await readBody(request);
  if (!validGrant(input, env)) return failure(400, 'invalid_request', 'Invalid token exchange parameters.');

  try {
    const response = await fetchUpstream(tokenEndpoint, {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded', Accept: 'application/json' },
      body: new URLSearchParams({ ...input, client_secret: env.TIKTOK_CLIENT_SECRET }),
      redirect: 'error',
      signal: AbortSignal.timeout(15000),
    });
    if (response.status === 429) return failure(429, 'rate_limit_exceeded', 'TikTok rate limit reached.');
    const result = await response.json();
    if (!response.ok || result?.error) {
      if (typeof result?.error === 'string' && Object.hasOwn(providerErrors, result.error)) {
        return failure(400, result.error, providerErrors[result.error]);
      }
      return failure(502, 'server_error', 'TikTok could not complete the token request.');
    }
    const { access_token, expires_in, refresh_token, refresh_expires_in, open_id, scope, token_type } = result;
    if (![access_token, refresh_token, open_id, scope].every(v => typeof v === 'string' && v.length > 0) ||
        ![expires_in, refresh_expires_in].every(v => Number.isSafeInteger(v) && v > 0) ||
        typeof token_type !== 'string' || token_type.toLowerCase() !== 'bearer') {
      return failure(502, 'server_error', 'Incomplete token response from TikTok.');
    }
    return json(200, { access_token, expires_in, refresh_token, refresh_expires_in, open_id, scope, token_type });
  } catch {
    return failure(502, 'server_error', 'TikTok token service did not respond as expected.');
  }
}

export default {
  fetch(request, env) { return handleRequest(request, env); },
};
