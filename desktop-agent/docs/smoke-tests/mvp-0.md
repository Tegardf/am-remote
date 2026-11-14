# MVP 0 smoke test (desktop-agent)

Needs a human on the real laptop (tray, firewall) and, for step 5, a phone on the same Wi-Fi.
`/status` reports `apple_music_running: false` until MVP 1 adds the SMTC backend.

1. `cargo run` in `desktop-agent/`.
   Expected: a red circle appears in the tray. Its menu shows `Pairing code: NNNNNN` (6 digits), `No paired devices` and `Quit`.
2. First run only: Windows Firewall asks about network access. Allow it on **Private** networks.
3. Pair from PowerShell (README "Try the API"), using the code from the tray:
   ```powershell
   $base = "http://localhost:8765/api/v1"
   $r = Invoke-RestMethod -Method Post "$base/pair" -ContentType 'application/json' -Body '{"code":"<code from tray>","device_name":"pc-test"}'
   $h = @{ Authorization = "Bearer $($r.token)" }
   Invoke-RestMethod "$base/status" -Headers $h
   ```
   Expected: `$r.token` is 43 characters; `/status` returns `agent_version` `0.1.0` and `apple_music_running` `False`.
   The tray code changes at once (the code is single use), and the menu now lists `Paired devices (1)` with `Revoke pc-test`.
4. Send a wrong code 6 times (`"code":"000000"`).
   Expected: `403` `BAD_PAIRING_CODE` every time. The 6th is refused because of the lock, so even the right code is refused for 60 s, then pairing works again.
5. On the phone, open any mDNS/Bonjour browser app (or the Flutter app once its MVP 0 exists).
   Expected: `_amremote._tcp` shows the laptop's hostname, port 8765, TXT `version=1`, `name=<hostname>` and `ip=<the laptop's LAN IPv4 address>`.
   Address change (optional, needs a second network): with the agent running, switch the laptop to another Wi-Fi (or renew its DHCP lease). Expected: within about 30 s the browser shows the new `ip`, and the agent's console logs "LAN address changed: mDNS `ip` updated".
6. Tray menu, `Paired devices`, `Revoke pc-test`.
   Expected: the device disappears from the menu, and the same token now gets `401 UNAUTHORIZED` on `/status`.
7. Error bodies are JSON, never plain text:
   ```powershell
   curl.exe -i http://localhost:8765/api/v1/nope                      # 401 without a token
   curl.exe -i -X POST http://localhost:8765/api/v1/pair -d "{ bad"   # 400 BAD_REQUEST
   ```
8. Tray, `Quit`. Expected: the process exits and port 8765 is free.
9. Check `%APPDATA%\am-remote\config.json` exists and lists the paired device. It contains tokens: don't share it.

Needs you (not covered by automated tests): the tray menu and firewall prompt (steps 1, 2, 6, 8), mDNS seen from the phone (step 5).
