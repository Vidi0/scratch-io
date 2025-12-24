# Itch.io API Reference

Documentation about itch.io API's version 2

More documentation is available here:
<https://itchapi.ryhn.link/API/V2/index.html>

## Base URL

`https://api.itch.io/`

To ensure the use of the V2 endpoint if a new version ever releases,
set the Accept request header to application/vnd.itch.v2
[(go-itchio#4)](https://github.com/itchio/go-itchio/issues/4). 

## Authentication

### Login

Obtain an API key using a username and password.
The API key is the `key` field of the successful response

#### Endpoint:

`POST https://api.itch.io/login`


#### Parameters:

- `username` (string): The username or email
- `password` (string): The password
- `source` (string): Any of `desktop`, `android`
- `force_recaptcha` (bool): Whether to force a recaptcha
- `recaptcha_response` (string, optional): The response token from
`https://itch.io/captcha`, if required

#### Response:

If login is successful:
```json
{"key":{"created_at":"2025-11-02T15:39:00.000000000Z","key":"REDACTED","source":"desktop","revoked":false,"last_used_at":"2025-12-24T18:45:56.000000000Z","user_id":11681540,"source_version":"26.1.9","updated_at":"2025-11-30T21:19:37.000000000Z","id":3329825},"success":true,"cookie":{"itchio":"REDACTED"}}
```
```

If the username or password is incorrect:
```json
{"errors":["Incorrect username or password"]}
```

If a recaptcha verification is needed:
```json
{"recaptcha_needed":true,"recaptcha_url":"https:\/\/itch.io\/captcha","success":false}
```

If TOTP verification is enabled:
```json
{"success":false,"token":"eyJzb3VyY2UiOiJkZXNrdG9wIiwibWV0aG9kIjoidG90cCIsImlkIjoxMTY4MTU0MCwiZXhwaXJlcyI6MTc2NjYwMjM0OX0=REDACTED","totp_needed":true}
```

### TOTP Verification

Finish login when TOTP is required.

#### Endpoint:

`POST https://api.itch.io/totp/verify`

#### Parameters:

- `token` (string): The TOTP token obtained from the login endpoint
- `code` (string): The verification code from the TOTP app

#### Response:

If login is successful:
```json
{"key":{"created_at":"2025-11-02T15:39:00.000000000Z","key":"REDACTED","source":"desktop","revoked":false,"last_used_at":"2025-12-24T18:45:56.000000000Z","user_id":11681540,"source_version":"26.1.9","updated_at":"2025-11-30T21:19:37.000000000Z","id":3329825},"success":true,"cookie":{"itchio":"REDACTED"}}
```

If the TOTP token timed out:
```json
{"errors":["two-factor login attempt timed out"]}
```

If the TOTP code is invalid:
```json
{"errors":["invalid code"]}
```
