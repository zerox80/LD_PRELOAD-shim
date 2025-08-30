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
```

## Verify after login

- Behavior: Scrolling should change globally in GNOME apps.
- Check that gnome-shell mapped the shim and picked up variables:

```bash
pid=$(pgrep -u "$USER" -x gnome-shell | head -n1)
echo "PID=$pid"
tr '\0' '\n' </proc/$pid/environ | egrep '^(LD_PRELOAD|SCROLL_)'
grep -F liblibinput_scroll_shim.so /proc/$pid/maps && echo "shim mapped"
```

- If `SCROLL_DEBUG=1` is set, see logs:

```bash
journalctl --user -b | grep libinput_scroll_shim | tail -n 50
```

## Tuning

- Base factors:
  - `SCROLL_SCALE` (global, default 1.0)
  - `SCROLL_SCALE_Y`, `SCROLL_SCALE_X` (axis overrides)
- Source multipliers (multiply with base):
  - `SCROLL_SCALE_FINGER`, `SCROLL_SCALE_WHEEL`, `SCROLL_SCALE_CONTINUOUS`
- Examples:

```ini
SCROLL_SCALE_Y=0.3          # Stronger vertical reduction
SCROLL_SCALE_FINGER=0.6     # Additional damping for touchpad two-finger
SCROLL_DEBUG=1              # Enable logs
SCROLL_DISABLE=1            # Emergency off switch
```

After changing `Environment=` entries in the GNOME Shell drop-in, re-login.

## GNOME Shell–only activation (details)

Some distros expose different unit names. Use this to discover the correct one, then create the drop-in:

```bash
systemctl --user list-units | grep -E "org\.gnome\.Shell|gnome-shell"
UNIT=org.gnome.Shell@wayland.service   # set to the value from the list

systemctl --user edit "$UNIT"
# In the editor (override file), add or update:
[Service]
Environment=LD_PRELOAD=/home/<your-username>/.local/lib/liblibinput_scroll_shim.so
# Optional tuning:
Environment=SCROLL_SCALE_Y=0.5
# Optional source multipliers:
# Environment=SCROLL_SCALE_FINGER=0.8
# Environment=SCROLL_SCALE_WHEEL=0.8
# Environment=SCROLL_DEBUG=1

systemctl --user daemon-reload
# Re-login to apply
```

### Uninstall / Rollback (GNOME Shell drop-in)

```bash
# Find your unit again (if needed)
systemctl --user list-units | grep -E "org\.gnome\.Shell|gnome-shell"
UNIT=org.gnome.Shell@wayland.service

# Remove the drop-in override and reload
rm -rf "$HOME/.config/systemd/user/${UNIT}.d"
systemctl --user daemon-reload
# Re-login
```

## Optional alternatives (not Shell-only)

- Per-user environment.d (may be unreliable for GNOME Shell on some distros):

```bash
install -Dm755 target/release/liblibinput_scroll_shim.so ~/.local/lib/liblibinput_scroll_shim.so
mkdir -p ~/.config/environment.d
cat > ~/.config/environment.d/99-scrollscale.conf <<'EOF'
LD_PRELOAD=/home/<your-username>/.local/lib/liblibinput_scroll_shim.so
SCROLL_SCALE_Y=0.5
# SCROLL_DEBUG=1
EOF
# Re-login
```

- System-wide environment.d (applies to all user sessions, not Shell-only):

```bash
sudo install -Dm755 target/release/liblibinput_scroll_shim.so /usr/local/lib/liblibinput_scroll_shim.so
echo 'LD_PRELOAD=/usr/local/lib/liblibinput_scroll_shim.so
SCROLL_SCALE_Y=0.5' | sudo tee /usr/lib/environment.d/99-scrollscale.conf >/dev/null
# Re-login
```

## Troubleshooting

- No effect after relogin:
  - Verify the `LD_PRELOAD` path exists and is readable.
  - Confirm `gnome-shell` has the shim mapped (see Verify).
  - Ensure you are on a Wayland session (`echo $XDG_SESSION_TYPE` → `wayland`).
- ld.so warnings like “cannot be preloaded … ignored” in terminals:
  - Typically caused by setting `LD_PRELOAD` globally to a non-existent path. Prefer the GNOME Shell drop-in method above, or fix the path:
    ```bash
    # Clear any user-manager LD_PRELOAD
    systemctl --user unset-environment LD_PRELOAD

    # Find and correct global/user configs that set the wrong path
    grep -nH 'LD_PRELOAD' ~/.config/environment.d/*.conf /usr/lib/environment.d/*.conf /etc/environment 2>/dev/null || true
    # Edit to use an absolute, correct path (no $USER/$HOME)
    ```
- Disable quickly:
  - In the drop-in, set `Environment=SCROLL_DISABLE=1` or temporarily remove the `Environment=LD_PRELOAD=...` line and re-login.
- No logs with `SCROLL_DEBUG=1`:
  - Ensure `SCROLL_DEBUG=1` is set in the same drop-in used by GNOME Shell and re-login.

## Notes

- Wayland focus: GNOME uses libinput → shim applies globally to mice, touchpads, trackballs, and high‑res wheels.
- Hooks: both `libinput_event_pointer_get_axis_value[_v120]()` and `libinput_event_pointer_get_scroll_value[_v120]()` are intercepted.
- Source-specific scaling is multiplied with the base (axis/global) factor.
- If libinput changes symbol names/versions in the future, rebuild/update may be necessary.
- Security: `LD_PRELOAD` is ignored for setuid binaries.

### Distro notes

- Ubuntu/Debian: libinput SONAME is usually `libinput.so.10` (the shim resolves via `RTLD_NEXT` first, then common SONAMEs).
- Fedora/Arch: similar SONAMEs; if resolution fails, please report your libinput version.
