ALPHAHEDGE RMS: running it on the VPS
======================================

This folder is safe to upload: it has no passwords and no data in it.

FIRST TIME
  1. In DuckDNS, make a SECOND name (for example rms-alphahedge) and point it at this VPS's IP, the
     same IP MarketHub's name uses.
  2. Copy this whole folder to the VPS (for example C:\AlphahedgeRMS), next to or apart from MarketHub's
     folder. Do not put it inside MarketHub's folder.
  3. Put the new DuckDNS name in domain.txt (one line, no #).
  4. Optional but recommended: put your own IP in allow.txt so only you can open the sign-in page.
  5. Double-click "Alphahedge RMS Server.bat". It asks for administrator rights, then:
       - creates the settings and database and shows the ADMIN PASSWORD ONCE (write it down),
       - starts the RMS and keeps it running with Windows,
       - adds the RMS website to the Caddy that MarketHub already uses,
       - schedules a daily backup at 03:30.
  6. Open https://<your RMS name> and sign in.

WHAT IT DOES NOT TOUCH
  MarketHub's files, its scheduled tasks, its port, its own website, or the firewall (ports 80 and 443
  are already open for MarketHub). The RMS listens on 127.0.0.1:8090, this computer only.
  The only change in MarketHub's Caddy folder is one new file: caddy\sites\rms.caddy.
  Before Caddy is told to load it, the whole Caddy configuration is checked; if the check fails the new
  file is removed again and nothing is reloaded. A Caddy reload is graceful (MarketHub's pages stay up),
  but a live connection that is open at that moment can drop once and reconnect by itself. To be sure,
  run the .bat after market hours, or run it as:  "Alphahedge RMS Server.bat" -NoReload
  and reload Caddy later.

UPDATING
  On the PC run the package script again, then copy rms\rms-server.exe and the rms\web folder over the
  ones on the VPS (or unpack the whole new zip over this folder: .env, data\, backups\ and the logs
  stay). Run "Alphahedge RMS Server.bat" again: it replaces the program only if it changed, applies any
  database upgrade, and restarts it.

THE C++ RUNTIME
  The RMS's database engine is C++ and needs Microsoft's Visual C++ runtime, version 14.40 or newer. The
  installer checks it and installs or upgrades it from Microsoft if needed.

FILES THAT LIVE ONLY ON THE VPS
  rms\.env       the admin password and the key that opens the saved broker logins. NEVER share it.
  rms\data\      the database (clients, accounts, positions, strategy tags).
  backups\       a daily copy of both, 14 days. Copy it off the VPS now and then: a database without
                 its .env cannot be read.
  rms.log        what the RMS prints; rms-backup.log for the backup.

IF YOU FORGET THE PASSWORD
  On the sign-in page press "Forgot password?" then "Make a reset code". The code is written to
  rms\data\reset-code.txt on the VPS (open it over Remote Desktop). Or read RMS_ADMIN_PASSWORD in rms\.env.

SECURITY NOTES
  - Five wrong passwords lock that address out for 15 minutes.
  - The login cookie is marked Secure when it comes over HTTPS.
  - Some brokers want the IP that calls their API to be registered: this VPS's IP is the one to register.
