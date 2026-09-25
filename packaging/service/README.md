# Unpeel Host service units

Start `supercli serve` (the UI-free Unpeel Host service) on boot, per user.
These files are the templates `supercli serve install` renders; they are also
usable verbatim in a container or golden image when `supercli` is installed at
`/usr/local/bin/supercli` (what `curl -fsSL https://supercli.com/install.sh | sh`
does).

The easy path on the Host machine itself:

```sh
supercli serve install     # write the unit, enable it, start it
supercli serve status      # unit state + live Host service status
supercli serve uninstall   # stop the service and remove the unit only
```

`supercli serve install` resolves the running `supercli` binary's real path into
the unit. With no `SUPERCLI_HOME` it installs the machine service (one
supervisor, every registered workspace). With `--workspace NAME` (or an
`SUPERCLI_HOME` that is a registered workspace) it installs a scoped
single-workspace unit — the container/explicit-unit shape. `uninstall` stops
the service and deletes the unit file; it never touches `~/.supercli` data, and
running Session PTYs survive a service stop by design.

## macOS — per-user LaunchAgent (`com.supercli.serve.plist`)

Written to `~/Library/LaunchAgents/com.supercli.serve.plist` (scoped:
`com.supercli.serve.<workspace>.plist`).

This must stay a **per-user LaunchAgent**, never a root LaunchDaemon: the
service owns `~/.supercli`, the user Keychain, and the per-user machine lease.
Consequence for a headless Mac: enable **automatic login** for the hosting
user (System Settings ▸ Users & Groups) so the `gui/<uid>` launchd domain
exists after a reboot with no one at the keyboard.

Manual verbatim use:

```sh
cp com.supercli.serve.plist ~/Library/LaunchAgents/
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.supercli.serve.plist
```

## Linux — systemd user unit (`supercli-serve.service`)

Written to `~/.config/systemd/user/supercli-serve.service` (scoped:
`supercli-serve-<workspace>.service`). `supercli-serve@.service` is the manual
template-instance spelling of the scoped shape.

This is deliberately a `--user` unit for the same ownership reasons. For a
headless box the user manager must outlive login sessions:

```sh
sudo loginctl enable-linger <user>
```

Manual verbatim use:

```sh
cp supercli-serve.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now supercli-serve.service
```

## Linux — desktop-session variant (`supercli-serve-graphical.service`)

Graphical tools launched by agents need the Host inside
the desktop session: the engine reads the session's `DISPLAY` /
`WAYLAND_DISPLAY` and the accessibility (AT-SPI) bus on the session D-Bus.
`supercli serve install --graphical` writes this template instead of the
plain one (same file name, `supercli-serve.service`; scoped:
`supercli-serve-<workspace>.service`): it is `PartOf=` / `WantedBy=`
`graphical-session.target`, so it starts when the desktop session activates
and stops when it ends, inheriting the display the session manager imported
into the user manager. GNOME, KDE, and sway do that import and pull the
target in from their own session target. A hand-rolled session (an Xvfb
script, a streamed Xorg desktop such as a Box) has no session manager, and
`graphical-session.target` refuses manual start by design, so it uses the
checked-in `supercli-desktop-session.target` (`BindsTo=graphical-session.target`)
once its display is up:

```sh
cp supercli-desktop-session.target ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user import-environment DISPLAY XAUTHORITY
systemctl --user start supercli-desktop-session.target     # stop it when the session ends
```

An `ExecStartPre=` import inside the unit cannot substitute for that: it
runs with the user manager's environment, which is exactly what lacks the
display. `supercli serve status` prints the variant,
`graphical-session.target`'s state, and the desktop session (display plus
session bus) visible to the calling shell.

## Diagnostics

Service stdout is not the diagnostic surface (launchd discards it; systemd
journals it). Durable diagnostics: `~/.supercli/hooks/trace.log`, plus
`~/.supercli/host-service.json` (machine) and `<home>/serve.json` (workspace).
