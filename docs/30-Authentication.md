# 30 - Authentication

## 🔐 JWT-Based Authentication

### Sign Up

```bash
curl -X POST http://localhost:3000/v1/auth/signup \
  -H "Content-Type: application/json" \
  -d '{
    "email": "user@example.com",
    "password": "secure_password"
  }'
```

**Response:**
```json
{
  "success": true,
  "data": {
    "user": {"id": 1, "email": "user@example.com"},
    "token": "eyJhbGc...",
    "refresh_token": "eyJhbGc..."
  }
}
```

### Login

```bash
curl -X POST http://localhost:3000/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "email": "user@example.com",
    "password": "secure_password"
  }'
```

### Using Tokens

```bash
# Include token in Authorization header
curl http://localhost:3000/v1/query/users \
  -H "Authorization: Bearer <jwt_token>"
```

### Token Lifecycle

```
┌─────────────────────────────────────────────────────────────┐
│              TOKEN LIFECYCLE                                  │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  1. User logs in → Access Token (1 hour)                   │
│                   + Refresh Token (7 days)                   │
│                                                              │
│  2. Access token expires → Use refresh token                 │
│                                                              │
│  3. Refresh token expires → Re-login required                │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Security Features

- ✅ Argon2id password hashing
- ✅ JWT token validation
- ✅ Automatic token expiration
- ✅ Refresh token rotation (each `/v1/auth/refresh` call deletes the old session and issues a new refresh token)
- ✅ Access-token blacklisting on logout (revoked JWT `jti`s are rejected even before natural expiry)

### Other Account Endpoints

```
GET    /v1/auth/me                  # Current user profile
PUT    /v1/auth/user                # Update profile
POST   /v1/auth/change-password     # Change password
GET    /v1/auth/sessions            # List active sessions (refresh tokens)
DELETE /v1/auth/sessions/:id        # Revoke a specific session
```

## 🔑 Additional Sign-In Methods

Beyond email/password, the following are implemented and mounted under `/v1/auth`:

**OAuth / social login** — Google, GitHub, Discord, and Apple:
```
GET  /v1/auth/providers                # List configured OAuth providers
GET  /v1/auth/authorize/:provider      # Start OAuth flow
GET  /v1/auth/callback/:provider       # OAuth callback
GET  /v1/auth/accounts                 # List linked OAuth accounts (authenticated)
DELETE /v1/auth/accounts/:provider     # Unlink an OAuth account
```

**Magic link (passwordless email):**
```
POST /v1/auth/magic-link               # Request a magic link
GET  /v1/auth/magic-link/verify        # Verify magic link token
```

**Multi-factor authentication (TOTP + recovery codes):**
```
POST   /v1/auth/mfa/enroll             # Start MFA enrollment
POST   /v1/auth/mfa/verify             # Verify enrollment with a TOTP code
POST   /v1/auth/mfa/challenge          # Verify a TOTP code during login
POST   /v1/auth/mfa/recovery           # Use a recovery code
DELETE /v1/auth/mfa                    # Disable MFA
GET    /v1/auth/mfa/status             # Get MFA status
```

**Phone OTP:**
```
POST /v1/auth/phone/send               # Send a one-time code via SMS
POST /v1/auth/phone/verify             # Verify the code
```

**CAPTCHA:**
```
GET /v1/auth/captcha                   # Get captcha configuration
```

> Note: the codebase also contains SAML 2.0 SSO (`stackhouse/src/auth/saml.rs`), WebAuthn/passkey (`stackhouse/src/auth/webauthn.rs`), RBAC (`stackhouse/src/auth/rbac.rs`), device-trust and impersonation modules. As of this writing their routers are not mounted in `stackhouse/src/main.rs`, so they are not reachable over HTTP in the default server build — treat them as library code available for a deployment to wire in, not as active endpoints.

---

**Next:** [Row-Level Security](./31-Row-Level-Security.md)
