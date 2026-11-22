import assert from 'node:assert/strict';
import { test } from 'node:test';
import { handleRequest } from './worker.mjs';

const endpoint = 'https://broker.example/v1/tiktok/token';
const env = {
  TIKTOK_CLIENT_KEY: 'public-key',
  TIKTOK_CLIENT_SECRET: 'operator-only-secret',
  TIKTOK_REDIRECT_URI: 'http://127.0.0.1:3859/api/oauth/tiktok/callback',
};
const authorization = {
  client_key: env.TIKTOK_CLIENT_KEY, grant_type: 'authorization_code',
  code: 'code+with/special&characters', code_verifier: 'v'.repeat(64),
  redirect_uri: env.TIKTOK_REDIRECT_URI,
};
const refresh = { client_key: env.TIKTOK_CLIENT_KEY, grant_type: 'refresh_token', refresh_token: 'old-token' };
const tokens = {
  access_token: 'access', refresh_token: 'rotated', expires_in: 7200, refresh_expires_in: 300000,
  open_id: 'creator', scope: 'video.publish', token_type: 'Bearer',
};

function post(body, url = endpoint, headers = { 'Content-Type': 'application/json; charset=utf-8' }) {
  return new Request(url, { method: 'POST', headers, body: typeof body === 'string' ? body : JSON.stringify(body) });
}

for (const grant of [authorization, refresh]) {
  test('exchanges ' + grant.grant_type + ' using only fixed endpoint and configured secret', async () => {
    let calls = 0;
    const response = await handleRequest(post(grant), env, async (url, options) => {
      calls++;
      assert.equal(url, 'https://open.tiktokapis.com/v2/oauth/token/');
      assert.equal(options.redirect, 'error');
      assert.equal(options.method, 'POST');
      assert.equal(options.headers['Content-Type'], 'application/x-www-form-urlencoded');
      assert.deepEqual(Object.fromEntries(options.body), { ...grant, client_secret: env.TIKTOK_CLIENT_SECRET });
      return Response.json({ ...tokens, extra: env.TIKTOK_CLIENT_SECRET });
    });
    assert.equal(calls, 1);
    assert.equal(response.status, 200);
    assert.equal(response.headers.get('Cache-Control'), 'no-store');
    assert.deepEqual(await response.json(), tokens);
  });
}

test('rejects unsupported routes, methods, and malformed or unapproved grants before exchange', async () => {
  const cases = [
    [post(refresh, endpoint.replace('https:', 'http:')), 400], [post(refresh, endpoint + '/'), 404],
    [post(refresh, endpoint + '?x=y'), 404], [new Request(endpoint), 405],
    [post(refresh, endpoint, { 'Content-Type': 'text/plain' }), 415], [post('broken'), 400],
    [post({ ...authorization, client_key: 'other' }), 400], [post({ ...authorization, redirect_uri: 'https://other.example' }), 400],
    [post({ ...authorization, code_verifier: 'short' }), 400], [post({ ...authorization, code_verifier: '!'.repeat(43) }), 400],
    [post({ ...authorization, code_verifier: 'a'.repeat(129) }), 400], [post({ ...authorization, code: 123 }), 400],
    [post({ ...refresh, client_secret: 'client-supplied' }), 400], [post({ ...refresh, url: 'https://other.example' }), 400],
    [post({ ...refresh, grant_type: 'client_credentials' }), 400], [post({ ...refresh, refresh_token: '' }), 400],
    [post({ ...refresh, refresh_token: 'r'.repeat(18000) }), 400],
  ];
  let calls = 0;
  for (const [request, expected] of cases) {
    const response = await handleRequest(request, env, () => { calls++; });
    assert.equal(response.status, expected);
    assert.equal(response.headers.get('Cache-Control'), 'no-store');
  }
  assert.equal(calls, 0);
});

test('returns safe, stable errors without forwarding provider diagnostics or repeating the exchange', async () => {
  for (const [provider, expectedStatus, expectedCode] of [
    [() => Response.json({ error: 'invalid_grant', error_description: env.TIKTOK_CLIENT_SECRET }, { status: 400 }), 400, 'invalid_grant'],
    [() => Response.json({ error: env.TIKTOK_CLIENT_SECRET }, { status: 400 }), 502, 'server_error'],
    [() => Response.json({ ...tokens, scope: [] }), 502, 'server_error'],
    [() => Response.json({ ...tokens, expires_in: 0 }), 502, 'server_error'],
    [() => new Response(null, { status: 429 }), 429, 'rate_limit_exceeded'],
    [() => new Response('not JSON', { status: 500 }), 502, 'server_error'],
    [() => { throw new Error(env.TIKTOK_CLIENT_SECRET); }, 502, 'server_error'],
  ]) {
    let calls = 0;
    const response = await handleRequest(post(refresh), env, () => { calls++; return provider(); });
    assert.equal(response.status, expectedStatus);
    assert.equal(calls, 1);
    assert.equal(response.headers.get('Cache-Control'), 'no-store');
    const body = await response.json();
    assert.equal(body.error, expectedCode);
    assert.ok(!JSON.stringify(body).includes(env.TIKTOK_CLIENT_SECRET));
  }
});

test('refuses incomplete operator configuration', async () => {
  let calls = 0;
  const response = await handleRequest(post(refresh), { ...env, TIKTOK_CLIENT_SECRET: '' }, () => { calls++; });
  assert.equal(response.status, 503);
  assert.equal(calls, 0);
});
