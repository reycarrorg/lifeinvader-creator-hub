# TikTok OAuth token broker

The TikTok app operator hosts this optional Worker. It exchanges a creator's authorization code and PKCE verifier, or refreshes that creator's token, using the operator's TikTok client secret. Tokens are returned to the desktop client. This service is stateless and has no publishing endpoint.

## Operator setup

1. Configure Login Kit for Desktop in your TikTok developer app. Register the exact loopback callback shown in the Hub, including the port and path. Assign a separate endpoint or deployment to a different client key or callback.
2. Create an HTTPS service with a module Worker runtime, such as Cloudflare Workers. Use `worker.mjs` as its module entry point. Set the following bindings in the host's configuration.

   | Binding | Value |
   | --- | --- |
   | `TIKTOK_CLIENT_KEY` | The public key for the same registered app the Hub authorizes. |
   | `TIKTOK_CLIENT_SECRET` | The app secret, stored with the host's secret manager. |
   | `TIKTOK_REDIRECT_URI` | The full redirect URI registered with TikTok and used by the Hub. |

3. Apply an edge rate limit to `POST /v1/tiktok/token` before enabling the route. Bound requests per source IP and across the endpoint according to expected creator traffic and provider quotas. Keep HTTP request and response bodies out of logs, traces, and error capture. The Worker makes one provider request per exchange. A timeout can occur after TikTok consumes a code or rotates a refresh token.
4. Set `SNAPMATIC_TIKTOK_BROKER_HOST` to the HTTPS hostname (for example `auth.example.com`) and `SNAPMATIC_TIKTOK_CLIENT_KEY` to the public key. Set them when building the desktop release to embed the public settings, or when launching Snapmatic to configure one installation. The Worker accepts requests at `/v1/tiktok/token`. Keep the client secret in the Worker's secret store. Public Direct Post also requires TikTok's audit and the creator's `video.publish` authorization.

## Request and response

Send a JSON object with `Content-Type: application/json`. For `authorization_code`, include `grant_type`, `client_key`, `code`, `code_verifier`, and `redirect_uri`. For `refresh_token`, include `grant_type`, `client_key`, and `refresh_token`.

A successful exchange returns `access_token`, `refresh_token`, `expires_in`, `refresh_expires_in`, `token_type`, `open_id`, and `scope`. Replace stored tokens after each successful refresh. Errors return a stable `error` code and a short `error_description`. The desktop client must protect the returned account credentials. TikTok's hosted backend model stores creator tokens server-side. [TikTok token management](https://developers.tiktok.com/docs/en/oauth-user-access-token-management)

## Offline tests

From the repository root with Node.js 22 or later:

```shell
node --test integrations/tiktok-oauth/worker.test.mjs
```

Tests use synthetic credentials and an injected TikTok response.
