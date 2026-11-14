---
paths:
  - "lib/core/**"
  - "lib/features/connect/**"
  - "android/app/src/main/**"
---

# Security (token, pairing, network)

- **Token storage:** only in `flutter_secure_storage`. Never in `shared_preferences`, logs, debug prints, crash or analytics reports, or test fixtures (tests use obviously fake tokens like the one in `../contract/samples.md`).
- **On `401 UNAUTHORIZED`:** delete the stored token, close the WebSocket, go back to pairing.
- **Never log secrets:** no tokens, no `Authorization` headers, no WebSocket URLs (they carry `?token=`). If you add a dio `LogInterceptor`, keep it in debug builds only with `requestHeader: false`.
- **Cleartext HTTP is only for the paired agent on the LAN.** `network_security_config.xml` allows cleartext because LAN IPs can't use TLS here, but the code must only send HTTP to the discovered or entered agent host. Anything else (e.g. `artwork_url` from search results) must be HTTPS.
- **Pairing input:** accept exactly 6 digits. Show `BAD_PAIRING_CODE` as "wrong or expired code", without echoing the code.
- **Manifest:** request only the permissions listed in ./CLAUDE.md. Keep `android:allowBackup="false"` so the token isn't included in backups.
