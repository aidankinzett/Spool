import customtkinter as ctk
from tkinter import filedialog
import json
import os
import re
import shutil
import subprocess
import sys
import threading
import urllib.parse

import requests


def _config_path():
    if getattr(sys, "frozen", False):
        base = os.environ.get("APPDATA") or os.path.expanduser("~")
        directory = os.path.join(base, "ludusavi-wrap")
    else:
        directory = os.path.dirname(os.path.abspath(__file__))
    os.makedirs(directory, exist_ok=True)
    return os.path.join(directory, "config.json")


def _launchers_dir():
    d = os.path.join(os.environ.get("APPDATA", ""), "ludusavi-wrap", "launchers")
    os.makedirs(d, exist_ok=True)
    return d


def _covers_dir():
    d = os.path.join(os.environ.get("APPDATA", ""), "ludusavi-wrap", "covers")
    os.makedirs(d, exist_ok=True)
    return d


def _safe_filename(name):
    return re.sub(r'[\\/:*?"<>|]', "", name).strip()


CONFIG_PATH = _config_path()

SGDB_BASE = "https://www.steamgriddb.com/api/v2"
MIME_EXT = {"image/png": ".png", "image/jpeg": ".jpg", "image/webp": ".webp"}

VERSION = "1.0.3"
RELEASES_API = "https://api.github.com/repos/aidankinzett/ludusavi-wrap/releases/latest"
RELEASES_URL = "https://github.com/aidankinzett/ludusavi-wrap/releases/latest"

WINDOW_W = 660

def _show_touch_keyboard():
    for base in (os.environ.get("ProgramFiles", ""), os.environ.get("ProgramFiles(x86)", "")):
        tabtip = os.path.join(base, "Common Files", "microsoft shared", "ink", "TabTip.exe")
        if os.path.exists(tabtip):
            try:
                subprocess.Popen([tabtip])
            except OSError:
                pass
            return


BAT_TEMPLATE = (
    "@echo off\r\n"
    "setlocal\r\n"
    'set "LUDUSAVI={ludusavi_path}"\r\n'
    'set "GAME_NAME={game_name}"\r\n'
    'set "GAME_EXE={game_exe}"\r\n'
    'set "RESTORE_OUT=%TEMP%\\ludusavi_%RANDOM%.json"\r\n'
    "\r\n"
    'if not exist "%LUDUSAVI%" (\r\n'
    '    echo ERROR: ludusavi.exe not found at "%LUDUSAVI%"\r\n'
    "    pause\r\n"
    "    exit /b 1\r\n"
    ")\r\n"
    "\r\n"
    '"%LUDUSAVI%" restore --api --cloud-sync --force "%GAME_NAME%" > "%RESTORE_OUT%"\r\n'
    "\r\n"
    "powershell -NoProfile -NonInteractive -Command \""
    "$ErrorActionPreference='SilentlyContinue'; "
    "try {{ "
    "$j = Get-Content $env:RESTORE_OUT | ConvertFrom-Json; "
    "if ($j.errors.cloudConflict -ne $null -or $j.errors.cloudSyncFailed -ne $null) {{ "
    "Add-Type -AssemblyName PresentationFramework; "
    "$r = [System.Windows.MessageBox]::Show("
    "'Cloud sync conflict detected for ' + $env:GAME_NAME + '. Open Ludusavi to resolve?',"
    "'Ludusavi - Cloud Conflict','YesNo','Warning'); "
    "if ($r -eq 'Yes') {{ Start-Process $env:LUDUSAVI -ArgumentList 'gui' }}; "
    "exit 1 "
    "}} "
    "}} catch {{ }}"
    "\"\r\n"
    "\r\n"
    "set \"PS_RESULT=%errorlevel%\"\r\n"
    'del "%RESTORE_OUT%" 2>nul\r\n'
    'if "%PS_RESULT%"=="1" exit /b 1\r\n'
    "\r\n"
    'start /wait "" "%GAME_EXE%"\r\n'
    "\r\n"
    '"%LUDUSAVI%" backup --force "%GAME_NAME%"\r\n'
)


class Config:
    def __init__(self):
        self.data = {
            "ludusavi_path": "",
            "steamgriddb_enabled": False,
            "steamgriddb_api_key": "",
        }
        self._load()
        self._autodetect()

    def _autodetect(self):
        if self.data.get("ludusavi_path"):
            return
        candidates = []
        found = shutil.which("ludusavi") or shutil.which("ludusavi.exe")
        if found:
            candidates.append(found)
        if getattr(sys, "frozen", False):
            candidates.append(os.path.join(os.path.dirname(sys.executable), "ludusavi.exe"))
        for path in candidates:
            path = os.path.normpath(os.path.abspath(path))
            if os.path.isfile(path):
                self.data["ludusavi_path"] = path
                self.save()
                return

    def _load(self):
        if os.path.exists(CONFIG_PATH):
            try:
                with open(CONFIG_PATH) as f:
                    self.data.update(json.load(f))
            except Exception:
                pass

    def save(self):
        with open(CONFIG_PATH, "w") as f:
            json.dump(self.data, f, indent=2)

    def get(self, key, default=""):
        return self.data.get(key, default)

    def set(self, key, value):
        self.data[key] = value
        self.save()

    def ludusavi_ok(self):
        p = self.data.get("ludusavi_path", "")
        return bool(p) and os.path.isfile(p)


class SetupDialog(ctk.CTkToplevel):
    def __init__(self, parent, config, on_saved):
        super().__init__(parent)
        self.config = config
        self.on_saved = on_saved
        self.title("Settings")
        self.geometry("520x370")
        self.resizable(False, False)
        self.grab_set()

        # ── Ludusavi ──────────────────────────────────────────────────────────
        ctk.CTkLabel(self, text="Ludusavi",
                     font=ctk.CTkFont(size=14, weight="bold")).pack(anchor="w", padx=24, pady=(20, 6))

        row = ctk.CTkFrame(self, fg_color="transparent")
        row.pack(fill="x", padx=24)
        self._path_var = ctk.StringVar(value=config.get("ludusavi_path"))
        ctk.CTkEntry(row, textvariable=self._path_var, width=360).pack(side="left", padx=(0, 8))
        ctk.CTkButton(row, text="Browse", width=80, command=self._browse_ludusavi).pack(side="left")

        # ── SteamGridDB ───────────────────────────────────────────────────────
        ctk.CTkFrame(self, height=1).pack(fill="x", padx=24, pady=(18, 0))

        sgdb_hdr = ctk.CTkFrame(self, fg_color="transparent")
        sgdb_hdr.pack(fill="x", padx=24, pady=(12, 2))
        ctk.CTkLabel(sgdb_hdr, text="SteamGridDB Artwork",
                     font=ctk.CTkFont(size=14, weight="bold")).pack(side="left")
        self._sgdb_switch = ctk.CTkSwitch(sgdb_hdr, text="", width=44,
                                           command=self._on_sgdb_toggle,
                                           onvalue=True, offvalue=False)
        self._sgdb_switch.pack(side="right")
        if config.get("steamgriddb_enabled"):
            self._sgdb_switch.select()

        ctk.CTkLabel(self, text="Download cover images automatically when generating a wrapper.",
                     text_color="gray", font=ctk.CTkFont(size=11)).pack(anchor="w", padx=24, pady=(0, 10))

        key_row = ctk.CTkFrame(self, fg_color="transparent")
        key_row.pack(fill="x", padx=24)
        ctk.CTkLabel(key_row, text="API Key:").pack(side="left", padx=(0, 8))
        self._key_var = ctk.StringVar(value=config.get("steamgriddb_api_key"))
        self._key_entry = ctk.CTkEntry(key_row, textvariable=self._key_var, width=290, show="•")
        self._key_entry.pack(side="left", padx=(0, 8))
        ctk.CTkButton(key_row, text="Get Key", width=74,
                      command=lambda: os.startfile(
                          "https://www.steamgriddb.com/profile/preferences/api"
                      )).pack(side="left")

        ctk.CTkLabel(self, text="steamgriddb.com/profile/preferences/api",
                     text_color="gray", font=ctk.CTkFont(size=10)).pack(anchor="w", padx=24, pady=(4, 0))

        # ── Error + Save ──────────────────────────────────────────────────────
        self._err = ctk.CTkLabel(self, text="", text_color="red", font=ctk.CTkFont(size=12))
        self._err.pack(pady=(14, 0))

        ctk.CTkButton(self, text="Save & Continue", command=self._save).pack(pady=(8, 20))

        self._on_sgdb_toggle()

    def _on_sgdb_toggle(self):
        self._key_entry.configure(state="normal" if self._sgdb_switch.get() else "disabled")

    def _browse_ludusavi(self):
        path = filedialog.askopenfilename(
            parent=self,
            title="Select ludusavi.exe",
            filetypes=[("ludusavi", "ludusavi.exe"), ("Executables", "*.exe")],
        )
        if path:
            self._path_var.set(os.path.normpath(path))

    def _save(self):
        path = self._path_var.get().strip()
        if not os.path.isfile(path):
            self._err.configure(text="File not found — please browse to a valid ludusavi.exe")
            return
        if self._sgdb_switch.get() and not self._key_var.get().strip():
            self._err.configure(text="An API key is required when SteamGridDB is enabled")
            return
        self.config.set("ludusavi_path", path)
        self.config.set("steamgriddb_enabled", bool(self._sgdb_switch.get()))
        self.config.set("steamgriddb_api_key", self._key_var.get().strip())
        self.destroy()
        self.on_saved()


ARMOURY_STEPS = (
    "1.  Open Armoury Crate → Library → Manage Library\n"
    "2.  Add the game's .exe file to your library\n"
    "3.  Select the game → press X → Game Options → Game Info → Edit\n"
    "4.  Paste the Game Name into the title field\n"
    "5.  Paste the Launch CMD into the Launch CMD field\n"
    "6.  Add cover art manually (path shown below if downloaded)"
)


class SuccessDialog(ctk.CTkToplevel):
    def __init__(self, parent, game_name, bat_path, on_close):
        super().__init__(parent)
        self.title("Ready for Armoury Crate")
        self.resizable(False, False)
        self.grab_set()
        self._on_close = on_close
        self.protocol("WM_DELETE_WINDOW", self._close)

        ctk.CTkLabel(self, text="Ready for Armoury Crate",
                     font=ctk.CTkFont(size=15, weight="bold")).pack(pady=(20, 12))

        self._build_copy_row("Game Name", game_name)
        self._build_copy_row("Launch CMD", bat_path)

        ctk.CTkFrame(self, height=1).pack(fill="x", padx=20, pady=(8, 6))

        ctk.CTkLabel(self, text="Next Steps", anchor="w",
                     font=ctk.CTkFont(size=13, weight="bold")).pack(anchor="w", padx=20)
        ctk.CTkLabel(self, text=ARMOURY_STEPS, justify="left",
                     wraplength=460).pack(anchor="w", padx=20, pady=(4, 4))

        self._artwork_label = ctk.CTkLabel(self, text="", wraplength=460,
                                            font=ctk.CTkFont(size=11),
                                            text_color=("gray40", "gray60"))
        self._artwork_label.pack(anchor="w", padx=20, pady=(0, 4))

        btn_row = ctk.CTkFrame(self, fg_color="transparent")
        btn_row.pack(pady=(8, 20))
        ctk.CTkButton(btn_row, text="Open Launchers Folder", width=160,
                      command=lambda: os.startfile(_launchers_dir())).pack(side="left", padx=(0, 8))
        ctk.CTkButton(btn_row, text="New Wrapper", width=110,
                      fg_color="transparent", border_width=1,
                      text_color=("gray10", "gray90"),
                      command=self._close).pack(side="left")

        self.update_idletasks()
        self.geometry(f"500x{self.winfo_reqheight()}")

    def _build_copy_row(self, label, value):
        ctk.CTkLabel(self, text=label, anchor="w",
                     font=ctk.CTkFont(size=12, weight="bold")).pack(anchor="w", padx=20, pady=(0, 3))
        row = ctk.CTkFrame(self, fg_color="transparent")
        row.pack(fill="x", padx=20, pady=(0, 10))
        entry = ctk.CTkEntry(row, font=ctk.CTkFont(size=12))
        entry.pack(side="left", fill="x", expand=True, padx=(0, 8))
        entry.insert(0, value)
        entry.configure(state="readonly")
        btn = ctk.CTkButton(row, text="Copy", width=70,
                            command=lambda: self._copy(value, btn))
        btn.pack(side="left")

    def _copy(self, text, btn):
        self.clipboard_clear()
        self.clipboard_append(text)
        self.update()
        btn.configure(text="✓")
        self.after(1500, lambda: btn.configure(text="Copy"))

    def update_artwork(self, text, color):
        if self.winfo_exists():
            self._artwork_label.configure(text=text, text_color=color)
            self.update_idletasks()
            self.geometry(f"500x{self.winfo_reqheight()}")

    def _close(self):
        self.destroy()
        self._on_close()


class App(ctk.CTk):
    def __init__(self, config):
        super().__init__()
        self.config = config
        self.title("Ludusavi Wrap")
        self.resizable(False, False)
        self._build()
        self.update_idletasks()
        self._form_height = self.winfo_reqheight()
        self.geometry(f"{WINDOW_W}x{self._form_height}")
        self.bind_all("<FocusIn>", self._on_focus_in)
        threading.Thread(target=self._check_for_update, daemon=True).start()

    def _build(self):
        # ── Header ──────────────────────────────────────────────────────────
        hdr = ctk.CTkFrame(self, fg_color="transparent")
        hdr.pack(fill="x", padx=20, pady=(18, 2))
        ctk.CTkLabel(hdr, text="Ludusavi Wrap",
                     font=ctk.CTkFont(size=20, weight="bold")).pack(side="left")
        ctk.CTkButton(hdr, text="Settings", width=80,
                      command=self._open_settings).pack(side="right")
        ctk.CTkLabel(self, text="Generate a save-managed launcher .bat for Armoury Crate",
                     text_color="gray").pack(anchor="w", padx=20)

        # ── Update banner (hidden until a newer release is found) ────────────
        self._update_banner = ctk.CTkFrame(
            self, fg_color=("#cfe2ff", "#1a2f4a"),
            border_width=1, border_color=("#6ea8fe", "#3a6fc4"),
        )
        banner_row = ctk.CTkFrame(self._update_banner, fg_color="transparent")
        banner_row.pack(fill="x", padx=12, pady=8)
        self._update_label = ctk.CTkLabel(
            banner_row, text="", font=ctk.CTkFont(size=12),
            text_color=("#084298", "#90c4ff"),
        )
        self._update_label.pack(side="left")
        ctk.CTkButton(
            banner_row, text="Download", width=80, height=26,
            font=ctk.CTkFont(size=12),
            command=lambda: os.startfile(RELEASES_URL),
        ).pack(side="right")

        ctk.CTkFrame(self, height=2).pack(fill="x", padx=20, pady=12)

        # ── Form ─────────────────────────────────────────────────────────────
        LW = 110  # fixed label column width keeps inputs aligned

        r1 = ctk.CTkFrame(self, fg_color="transparent")
        r1.pack(fill="x", padx=20, pady=(0, 8))
        ctk.CTkLabel(r1, text="Executable", width=LW, anchor="w").pack(side="left", padx=(0, 8))
        self._exe_var = ctk.StringVar()
        ctk.CTkEntry(r1, textvariable=self._exe_var, state="readonly").pack(side="left", fill="x", expand=True, padx=(0, 8))
        ctk.CTkButton(r1, text="Browse", width=80, command=self._browse_exe).pack(side="left")

        r2 = ctk.CTkFrame(self, fg_color="transparent")
        r2.pack(fill="x", padx=20, pady=(0, 0))
        ctk.CTkLabel(r2, text="Game Name", width=LW, anchor="w").pack(side="left", padx=(0, 8))
        self._name_var = ctk.StringVar()
        ctk.CTkEntry(r2, textvariable=self._name_var).pack(side="left", fill="x", expand=True, padx=(0, 8))
        self._search_btn = ctk.CTkButton(r2, text="Search", width=80, command=self._search)
        self._search_btn.pack(side="left")

        self._results_outer = ctk.CTkFrame(self, fg_color="transparent")
        self._results_outer.pack(fill="x", padx=20)
        self._results_box = ctk.CTkScrollableFrame(self._results_outer, height=80, label_text="")

        # ── Generate ────────────────────────────────────────────────────────
        self._generate_btn = ctk.CTkButton(self, text="Generate Wrapper", height=40,
                                           font=ctk.CTkFont(size=14, weight="bold"),
                                           command=self._generate)
        self._generate_btn.pack(pady=20)

        self._status = ctk.CTkLabel(self, text="", font=ctk.CTkFont(size=13), wraplength=500)

    # ── helpers ──────────────────────────────────────────────────────────────

    def _on_focus_in(self, event):
        if event.widget.winfo_class() == "Entry":
            _show_touch_keyboard()

    def _browse_exe(self):
        path = filedialog.askopenfilename(
            parent=self, title="Select game executable",
            filetypes=[("Executable", "*.exe"), ("All files", "*.*")],
        )
        if not path:
            return
        path = os.path.normpath(path)
        self._exe_var.set(path)
        if not self._name_var.get():
            base = os.path.splitext(os.path.basename(path))[0]
            self._name_var.set(base.replace("_", " ").replace("-", " ").title())

    def _search(self):
        query = self._name_var.get().strip()
        if not query:
            return
        self._search_btn.configure(state="disabled", text="Searching…")
        threading.Thread(target=self._do_search, args=(query,), daemon=True).start()

    def _do_search(self, query):
        try:
            proc = subprocess.run(
                [self.config.get("ludusavi_path"), "find", "--api", "--fuzzy", "--multiple", query],
                capture_output=True, text=True, timeout=15,
            )
            games = list(json.loads(proc.stdout).get("games", {}).keys()) if proc.returncode == 0 else []
        except Exception:
            games = []
        self.after(0, self._show_results, games)

    def _show_results(self, games):
        self._search_btn.configure(state="normal", text="Search")
        for w in self._results_box.winfo_children():
            w.destroy()
        if games:
            self._results_box.pack(fill="x", pady=(4, 0))
            for g in games[:12]:
                ctk.CTkButton(
                    self._results_box, text=g, anchor="w",
                    fg_color="transparent", hover_color=("gray85", "gray25"),
                    text_color=("gray10", "gray90"),
                    command=lambda name=g: (self._name_var.set(name), self._results_box.pack_forget()),
                ).pack(fill="x", pady=1)
        else:
            self._results_box.pack_forget()

    def _generate(self):
        exe  = self._exe_var.get().strip()
        name = self._name_var.get().strip()

        if not exe:
            return self._set_status("Please select a game executable.", ok=False)
        if not name:
            return self._set_status("Please enter a Ludusavi game name.", ok=False)
        if not self.config.ludusavi_ok():
            return self._set_status("Ludusavi not found — open Settings to configure it.", ok=False)

        safe = _safe_filename(name)
        if not safe:
            return self._set_status("Game name contains only invalid filename characters.", ok=False)

        bat_path = os.path.join(_launchers_dir(), safe + ".bat")

        content = BAT_TEMPLATE.format(
            ludusavi_path=self.config.get("ludusavi_path"),
            game_name=name,
            game_exe=exe,
        )
        with open(bat_path, "w", newline="") as f:
            f.write(content)

        self._set_status("", ok=True)
        self._success_dlg = SuccessDialog(self, name, bat_path, on_close=self._clear)

        if self.config.get("steamgriddb_enabled") and self.config.get("steamgriddb_api_key"):
            self._success_dlg.update_artwork("Fetching cover image…", ("gray40", "gray60"))
            threading.Thread(target=self._fetch_hero, args=(name, safe, bat_path), daemon=True).start()

    def _fetch_hero(self, game_name, safe_name, bat_path):
        api_key = self.config.get("steamgriddb_api_key")
        headers = {"Authorization": f"Bearer {api_key}"}
        try:
            encoded = urllib.parse.quote(game_name)
            resp = requests.get(f"{SGDB_BASE}/search/autocomplete/{encoded}",
                                headers=headers, timeout=10)
            resp.raise_for_status()
            results = resp.json().get("data", [])
            if not results:
                self.after(0, self._on_artwork_error, "Game not found on SteamGridDB")
                return

            game_id = results[0]["id"]

            resp2 = requests.get(f"{SGDB_BASE}/grids/game/{game_id}",
                                 headers=headers, timeout=10,
                                 params={"dimensions": "460x215,920x430"})
            resp2.raise_for_status()
            grids = resp2.json().get("data", [])
            if not grids:
                self.after(0, self._on_artwork_error, "No horizontal grid images found on SteamGridDB")
                return

            img_url = grids[0]["url"]
            mime = grids[0].get("mime", "image/jpeg")
            ext = MIME_EXT.get(mime, os.path.splitext(img_url)[1] or ".jpg")

            img_resp = requests.get(img_url, timeout=20)
            img_resp.raise_for_status()

            img_path = os.path.join(_covers_dir(), f"{safe_name}{ext}")
            with open(img_path, "wb") as f:
                f.write(img_resp.content)

            self.after(0, self._on_artwork_done, img_path)

        except Exception as e:
            self.after(0, self._on_artwork_error, str(e))

    def _on_artwork_done(self, img_path):
        if hasattr(self, "_success_dlg") and self._success_dlg.winfo_exists():
            self._success_dlg.update_artwork(
                f"Cover art: {img_path}",
                ("#155724", "#4caf50"),
            )

    def _on_artwork_error(self, msg):
        if hasattr(self, "_success_dlg") and self._success_dlg.winfo_exists():
            self._success_dlg.update_artwork(
                f"⚠ Artwork: {msg}",
                ("#856404", "#ffc107"),
            )

    def _set_status(self, msg, ok=True):
        self._status.configure(text=msg, text_color="green" if ok else "red")
        if msg:
            self._status.pack(padx=20, pady=(0, 20))
        else:
            self._status.pack_forget()
        self.geometry(f"{WINDOW_W}x{self.winfo_reqheight()}")

    def _clear(self):
        self._exe_var.set("")
        self._name_var.set("")
        for w in self._results_box.winfo_children():
            w.destroy()
        self._results_box.pack_forget()
        self._set_status("", ok=True)

    def _check_for_update(self):
        try:
            resp = requests.get(RELEASES_API, timeout=8,
                                headers={"Accept": "application/vnd.github+json"})
            resp.raise_for_status()
            latest = resp.json().get("tag_name", "").lstrip("v")
            if latest and self._is_newer(latest, VERSION):
                self.after(0, self._show_update_banner, latest)
        except Exception:
            pass

    @staticmethod
    def _is_newer(latest, current):
        def parts(v):
            return tuple(int(x) for x in v.split("."))
        try:
            return parts(latest) > parts(current)
        except ValueError:
            return False

    def _show_update_banner(self, latest):
        self._update_label.configure(text=f"v{latest} is available")
        self._update_banner.pack(fill="x", padx=20, pady=(8, 0))
        self.update_idletasks()
        self.geometry(f"{WINDOW_W}x{self.winfo_reqheight()}")

    def _open_settings(self):
        dlg = SetupDialog(self, self.config, lambda: None)
        self.wait_window(dlg)


def main():
    ctk.set_appearance_mode("System")
    _base = getattr(sys, "_MEIPASS", os.path.dirname(os.path.abspath(__file__)))
    ctk.set_default_color_theme(os.path.join(_base, "themes", "rime.json"))

    config = Config()
    app = App(config)

    if not config.ludusavi_ok():
        app.withdraw()

        def on_saved():
            app.deiconify()

        def on_cancel():
            app.destroy()

        dlg = SetupDialog(app, config, on_saved)
        dlg.protocol("WM_DELETE_WINDOW", on_cancel)

    app.mainloop()


if __name__ == "__main__":
    main()
