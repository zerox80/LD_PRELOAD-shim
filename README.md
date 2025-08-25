# libinput_scroll_shim

LD_PRELOAD shim that scales libinput scroll deltas globally inside GNOME/Mutter (Wayland). Works for mice and touchpads because scaling happens after libinput normalization.

- Scales vertical/horizontal scroll values from `libinput_event_pointer_get_axis_value()` and `libinput_event_pointer_get_scroll_value()` (and their `_v120` variants)
- Optional per-source multipliers (wheel vs. finger vs. continuous)
- Controlled via environment variables; safe fallback (scale=1.0 if unset)

## Quick start (new PC, GNOME Wayland)

Prereqs: GNOME on Wayland, sudo available, Rust toolchain installed (`cargo`, `rustc`).

```bash
# 1) Build
cargo build --release

# 2) Install the .so to a stable path (user-local)
install -Dm755 target/release/liblibinput_scroll_shim.so ~/.local/lib/liblibinput_scroll_shim.so

# 3) Enable system-wide so that gnome-shell reliably inherits it
sudo install -Dm644 /dev/null /usr/lib/environment.d/99-scrollscale.conf
sudo sh -c 'cat > /usr/lib/environment.d/99-scrollscale.conf <<EOF
LD_PRELOAD=/home/$USER/.local/lib/liblibinput_scroll_shim.so
SCROLL_SCALE_Y=0.5
# Optional: tune touchpad finger separately (multiplies base)
# SCROLL_SCALE_FINGER=0.8
# Optional: debug logs to journalctl
# SCROLL_DEBUG=1
EOF'

# 4) Log out and back in (Wayland session)
```

Why system-wide? Some distros do not pass user `~/.config/environment.d` reliably to the shell login. Using `/usr/lib/environment.d/` ensures GNOME Shell gets `LD_PRELOAD` early.

Optional hardening: Place the .so in a system path so ld.so always finds it during early session startup.

```bash
sudo install -Dm755 target/release/liblibinput_scroll_shim.so /usr/local/lib/liblibinput_scroll_shim.so
sudo sed -i 's#^LD_PRELOAD=.*#LD_PRELOAD=/usr/local/lib/liblibinput_scroll_shim.so#' /usr/lib/environment.d/99-scrollscale.conf
# Log out and back in again
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

After changes in `/usr/lib/environment.d/99-scrollscale.conf` (or `~/.config/environment.d/...`), log out/in.

## Alternative: per-user activation

```bash
install -Dm755 target/release/liblibinput_scroll_shim.so ~/.local/lib/liblibinput_scroll_shim.so
mkdir -p ~/.config/environment.d
cat > ~/.config/environment.d/99-scrollscale.conf <<'EOF'
LD_PRELOAD=/home/$USER/.local/lib/liblibinput_scroll_shim.so
SCROLL_SCALE_Y=0.5
# SCROLL_DEBUG=1
EOF
# Log out and back in (may be unreliable on some distros for gnome-shell)
```

### GNOME Shell–only activation (advanced)

Some distros expose a user unit for gnome-shell. If present, you can inject env only for the shell:

```bash
systemctl --user list-units | grep -i shell
systemctl --user edit gnome-shell.service
# Add to the override:
[Service]
Environment=LD_PRELOAD=/usr/local/lib/liblibinput_scroll_shim.so
# Then relogin
```

## Troubleshooting

- No effect after relogin:
  - Verify `LD_PRELOAD` path exists and is readable.
  - Confirm `gnome-shell` has the shim mapped (see Verify).
  - Ensure you are on a Wayland session (`echo $XDG_SESSION_TYPE` → `wayland`).
- ld.so warnings like “cannot be preloaded … ignored” in early session:
  - Move the .so to `/usr/local/lib/` and point `LD_PRELOAD` there (see Quick start → Optional hardening).
- Crashes or want to disable quickly:
  - Set `SCROLL_DISABLE=1` or temporarily remove `LD_PRELOAD` from the environment.d file and relogin.
- No logs with `SCROLL_DEBUG=1`:
  - Ensure `SCROLL_DEBUG=1` is set in the same environment.d file that gnome-shell reads (system-wide or per-user) and relogin.

## Uninstall / Rollback

System-wide activation:

```bash
sudo rm -f /usr/lib/environment.d/99-scrollscale.conf
# Optionally remove the installed library
sudo rm -f /usr/local/lib/liblibinput_scroll_shim.so
rm -f ~/.local/lib/liblibinput_scroll_shim.so
# Relogin
```

Per-user activation:

```bash
rm -f ~/.config/environment.d/99-scrollscale.conf
rm -f ~/.local/lib/liblibinput_scroll_shim.so
# Relogin
```

## Notes

- Wayland focus: GNOME uses libinput → shim applies globally to mice, touchpads, trackballs, and high‑res wheels.
- Hooks: both `libinput_event_pointer_get_axis_value[_v120]()` and `libinput_event_pointer_get_scroll_value[_v120]()` are intercepted.
- Source-specific scaling is multiplied with the base (axis/global) factor.
- If libinput changes symbol names/versions in the future, rebuild/update may be necessary.
- Security: `LD_PRELOAD` is ignored for setuid binaries; GNOME Shell is unaffected.

### Distro notes

- Ubuntu/Debian: libinput SONAME is usually `libinput.so.10` (the shim resolves via `RTLD_NEXT` first, then common SONAMEs).
- Fedora/Arch: similar SONAMEs; if resolution fails, please report your libinput version.
