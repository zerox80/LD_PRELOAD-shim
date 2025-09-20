# libinput_scroll_shim

LD_PRELOAD shim that scales libinput scroll deltas globally inside GNOME/Mutter (Wayland). Works for mice and touchpads because scaling happens after libinput normalization.

- Scales vertical/horizontal scroll values from `libinput_event_pointer_get_axis_value()` and `libinput_event_pointer_get_scroll_value()` (and their `_v120` variants)
- Optional per-source multipliers (wheel vs. finger vs. continuous)
- Controlled via environment variables; safe fallback (scale=1.0 if unset)

## Quick start (GNOME Shell–only, Wayland)

This setup injects the shim only into GNOME Shell, which then propagates to apps it launches. It avoids setting global environment for all user processes.

```bash
# 1) Build
cargo build --release

# 2) Install the .so to a stable path (user-local)
install -Dm755 target/release/liblibinput_scroll_shim.so ~/.local/lib/liblibinput_scroll_shim.so

# 3) Detect your GNOME Shell user unit name
systemctl --user list-units | grep -E "org\.gnome\.Shell|gnome-shell"
# Typical: org.gnome.Shell@wayland.service
UNIT=org.gnome.Shell@wayland.service  # change if your list shows a different Shell unit

# 4) Create a drop-in for GNOME Shell with absolute paths (no $USER/$HOME)
systemctl --user edit "$UNIT"
# Add the following lines in the opened editor, then save and exit:
#
# [Service]
# Environment=LD_PRELOAD=/home/<your-username>/.local/lib/liblibinput_scroll_shim.so
# Environment=SCROLL_SCALE_Y=0.5

# 5) Reload user units and re-login (Wayland Shell cannot be safely restarted inline)
systemctl --user daemon-reload
# Now log out and log back in (Wayland session)
```

Important: Neither systemd drop-ins (`Environment=`) nor `environment.d` perform shell variable expansion. Do not use `$USER` or `$HOME`. Always provide an absolute path, for example `LD_PRELOAD=/home/rujbin/.local/lib/liblibinput_scroll_shim.so`.

Optional hardening: Place the .so in a system path so the dynamic loader finds it early and you can reference a path that never depends on the home directory.

```bash
sudo install -Dm755 target/release/liblibinput_scroll_shim.so /usr/local/lib/liblibinput_scroll_shim.so
# Then in your drop-in, set:
# Environment=LD_PRELOAD=/usr/local/lib/liblibinput_scroll_shim.so
# and relogin
