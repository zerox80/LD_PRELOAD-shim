# Libinput Scroll Shim Configurator

This Python utility automates building and configuring a Rust-based LD_PRELOAD shim to adjust libinput scroll sensitivity on Linux systems, specifically targeting Ubuntu GNOME.

**Prerequisites:**
* Rust/Cargo installed

**Usage:**
Run the script manually:
```bash
python3 run_shim.py
```

**Features:**
* **Builds Shim:** Automatically compiles the Rust project in release mode.
* **Custom Scroll Speed:** Prompts for a Y-axis velocity multiplier (supports negative values for inversion).
* **Two Modes:**
    1. **Test Mode:** Launches a specific application with the modified scroll behavior.
    2. **Global Install:** Generates a configuration file for persistent system-wide application.
