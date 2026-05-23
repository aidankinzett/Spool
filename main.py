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


def run_cli_wrapper(game_name, game_exe):
    import tkinter as tk
    import tkinter.messagebox as messagebox

    def show_error(title, msg):
        root = tk.Tk()
        root.withdraw()
        messagebox.showerror(title, msg, parent=root)
        root.destroy()

    def show_warning(title, msg):
        root = tk.Tk()
        root.withdraw()
        messagebox.showwarning(title, msg, parent=root)
        root.destroy()

    def ask_yes_no(title, msg):
        root = tk.Tk()
        root.withdraw()
        res = messagebox.askyesno(title, msg, parent=root)
        root.destroy()
        return res

    config = Config()
    ludusavi_path = config.get("ludusavi_path")
    if not ludusavi_path or not os.path.isfile(ludusavi_path):
        show_error(
            "Ludusavi Error",
            f"Ludusavi executable not found at:\n{ludusavi_path}\n\nPlease open settings in Ludusavi Wrap to configure it."
        )
        sys.exit(1)

    # Helper function to run a process with a loading GUI dialog
    def run_ludusavi_with_loading(args, label_text):
        result = {"proc": None, "error": None}

        def worker():
            try:
                result["proc"] = subprocess.run(
                    [ludusavi_path] + args,
                    capture_output=True,
                    text=True,
                    creationflags=subprocess.CREATE_NO_WINDOW
                )
            except Exception as e:
                result["error"] = e

        thread = threading.Thread(target=worker, daemon=True)
        thread.start()

        # Build dialog window using CustomTkinter
        ctk.set_appearance_mode("System")
        _base = getattr(sys, "_MEIPASS", os.path.dirname(os.path.abspath(__file__)))
        ctk.set_default_color_theme(os.path.join(_base, "themes", "rime.json"))

        root = ctk.CTk()
        root.title("Ludusavi Wrap")
        root.resizable(False, False)
        root.protocol("WM_DELETE_WINDOW", lambda: None)  # Prevent closing during operation

        ctk.CTkLabel(
            root, text=label_text,
            font=ctk.CTkFont(size=13, weight="bold")
        ).pack(pady=(20, 8), padx=40)

        pb = ctk.CTkProgressBar(root)
        pb.pack(pady=8, padx=40, fill="x")
        pb.configure(mode="indeterminate")
        pb.start()

        # Let Tkinter calculate required sizes and center on screen
        root.update_idletasks()
        req_width = max(420, root.winfo_reqwidth())
        req_height = max(130, root.winfo_reqheight())

        screen_width = root.winfo_screenwidth()
        screen_height = root.winfo_screenheight()
        x = (screen_width - req_width) // 2
        y = (screen_height - req_height) // 2
        root.geometry(f"{req_width}x{req_height}+{x}+{y}")
        root.attributes("-topmost", True)

        def check_thread():
            if thread.is_alive():
                root.after(100, check_thread)
            else:
                root.destroy()

        root.after(100, check_thread)
        root.mainloop()

        if result["error"]:
            raise result["error"]
        return result["proc"]

    # 1. Run Ludusavi restore with loading dialog
    try:
        proc = run_ludusavi_with_loading(
            ["restore", "--api", "--cloud-sync", "--force", game_name],
            f"Restoring saves for '{game_name}'..."
        )
        restore_out = proc.stdout + proc.stderr
        returncode = proc.returncode
    except Exception as e:
        show_error(
            "Ludusavi Error",
            f"Failed to start Ludusavi restore process:\n{e}"
        )
        sys.exit(1)

    if returncode != 0:
        show_error(
            "Ludusavi Error",
            f"Ludusavi restore failed. Game will not launch.\n\nDetails:\n{restore_out}"
        )
        sys.exit(1)

    # Check for cloud conflicts
    if "cloudConflict" in restore_out or "cloudSyncFailed" in restore_out:
        ans = ask_yes_no(
            "Ludusavi - Cloud Conflict",
            f"Cloud sync conflict detected for '{game_name}'. Open Ludusavi to resolve?"
        )
        if ans:
            try:
                subprocess.Popen([ludusavi_path, "gui"])
            except Exception as e:
                show_error("Error", f"Failed to open Ludusavi GUI:\n{e}")
        sys.exit(1)

    # 2. Run the game
    if not os.path.isfile(game_exe):
        show_error(
            "Game Launcher Error",
            f"Game executable not found at:\n{game_exe}"
        )
        sys.exit(1)

    try:
        game_dir = os.path.dirname(game_exe)
        subprocess.run([game_exe], cwd=game_dir)
    except Exception as e:
        show_error(
            "Game Launcher Error",
            f"Failed to start game:\n{e}"
        )
        sys.exit(1)

    # 3. Run Ludusavi backup with loading dialog
    try:
        proc = run_ludusavi_with_loading(
            ["backup", "--force", "--cloud-sync", game_name],
            f"Backing up saves for '{game_name}'..."
        )
        if proc.returncode != 0:
            show_warning(
                "Ludusavi Warning",
                "Ludusavi backup failed. Your saves may not have been uploaded to the cloud."
            )
    except Exception as e:
        show_warning(
            "Ludusavi Warning",
            f"Failed to run Ludusavi backup:\n{e}"
        )



class Config:
    def __init__(self):
        self.data = {
            "ludusavi_path": "",
            "steamgriddb_enabled": False,
            "steamgriddb_api_key": "",
            "ludusavi_wrap_exe": "",
            "ludusavi_wrap_args": "",
        }
        self._load()
        self._autodetect()
        self._save_current_exe_path()

    def _save_current_exe_path(self):
        if getattr(sys, "frozen", False):
            self.data["ludusavi_wrap_exe"] = sys.executable
            self.data["ludusavi_wrap_args"] = ""
        else:
            self.data["ludusavi_wrap_exe"] = sys.executable
            main_py = os.path.normpath(os.path.abspath(sys.argv[0]))
            self.data["ludusavi_wrap_args"] = main_py
        self.save()

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
    "1.  Open Armoury Crate → Library → Manage Library (Add Game)\n"
    "2.  Browse and select the generated launcher .exe\n"
    "3.  (Optional) Add cover art manually if downloaded (path shown below)"
)


class SuccessDialog(ctk.CTkToplevel):
    def __init__(self, parent, game_name, exe_path, on_close):
        super().__init__(parent)
        self.title("Ready for Armoury Crate")
        self.resizable(False, False)
        self.grab_set()
        self._on_close = on_close
        self.protocol("WM_DELETE_WINDOW", self._close)

        ctk.CTkLabel(self, text="Ready for Armoury Crate",
                     font=ctk.CTkFont(size=15, weight="bold")).pack(pady=(20, 12))

        self._build_copy_row("Game Name", game_name)
        self._build_copy_row("Launcher EXE Path", exe_path)

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

        base_dir = getattr(sys, "_MEIPASS", os.path.dirname(os.path.abspath(__file__)))
        stub_path = os.path.join(base_dir, "launcher_stub.exe")
        if not os.path.isfile(stub_path):
            return self._set_status(f"Launcher stub not found at: {stub_path}", ok=False)

        exe_path = os.path.join(_launchers_dir(), safe + ".exe")

        try:
            shutil.copy2(stub_path, exe_path)
        except Exception as e:
            return self._set_status(f"Failed to copy launcher stub: {e}", ok=False)

        ludusavi_wrap_exe = self.config.get("ludusavi_wrap_exe")
        payload = f"\r\nLUDUSAVI_WRAP_CFG_START\r\n{name}\r\n{exe}\r\n{ludusavi_wrap_exe}\r\nLUDUSAVI_WRAP_CFG_END\r\n"

        try:
            with open(exe_path, "ab") as f:
                f.write(payload.encode("utf-8"))
        except Exception as e:
            try:
                os.remove(exe_path)
            except OSError:
                pass
            return self._set_status(f"Failed to write launcher configuration: {e}", ok=False)

        self._set_status("", ok=True)
        self._success_dlg = SuccessDialog(self, name, exe_path, on_close=self._clear)

        if self.config.get("steamgriddb_enabled") and self.config.get("steamgriddb_api_key"):
            self._success_dlg.update_artwork("Fetching cover image…", ("gray40", "gray60"))
            threading.Thread(target=self._fetch_hero, args=(name, safe, exe_path), daemon=True).start()

    def _fetch_hero(self, game_name, safe_name, exe_path):
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
    if len(sys.argv) > 1 and sys.argv[1] == "--run":
        if len(sys.argv) >= 4:
            run_cli_wrapper(sys.argv[2], sys.argv[3])
        return

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
