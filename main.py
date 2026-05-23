import customtkinter as ctk
from tkinter import filedialog
import json
import os
import shutil
import subprocess
import sys
import threading
import urllib.parse

import requests


def _config_path():
    if getattr(sys, "frozen", False):
        # Running as a PyInstaller exe — write to AppData so the config
        # survives updates and isn't lost in the temp extraction folder.
        base = os.environ.get("APPDATA") or os.path.expanduser("~")
        directory = os.path.join(base, "ludusavi-wrap")
    else:
        directory = os.path.dirname(os.path.abspath(__file__))
    os.makedirs(directory, exist_ok=True)
    return os.path.join(directory, "config.json")


CONFIG_PATH = _config_path()

SGDB_BASE = "https://www.steamgriddb.com/api/v2"
MIME_EXT = {"image/png": ".png", "image/jpeg": ".jpg", "image/webp": ".webp"}

BAT_TEMPLATE = (
    "@echo off\r\n"
    "setlocal\r\n"
    'set "LUDUSAVI={ludusavi_path}"\r\n'
    'set "GAME_NAME={game_name}"\r\n'
    'set "GAME_EXE={game_exe}"\r\n'
    "\r\n"
    'if not exist "%LUDUSAVI%" (\r\n'
    '    echo ERROR: ludusavi.exe not found at "%LUDUSAVI%"\r\n'
    "    pause\r\n"
    "    exit /b 1\r\n"
    ")\r\n"
    "\r\n"
    '"%LUDUSAVI%" wrap --name "%GAME_NAME%" --force -- "%GAME_EXE%"\r\n'
)

ARMOURY_STEPS = (
    "1.  Open Armoury Crate → Library → Manage Library\n"
    "2.  Use L/R buttons to open File Explorer\n"
    "3.  Select the generated .bat file as the game\n"
    "4.  Press X → Game Options → Game Info → Edit\n"
    "5.  Paste the .bat path into Launch CMD\n"
    "6.  Set the game title and use the downloaded hero image for art"
)


class Config:
    def __init__(self):
        self.data = {
            "ludusavi_path": "",
            "default_output_folder": "",
            "steamgriddb_enabled": False,
            "steamgriddb_api_key": "",
        }
        self._load()
        self._autodetect()

    def _autodetect(self):
        if not self.data.get("ludusavi_path"):
            found = shutil.which("ludusavi")
            if found:
                self.data["ludusavi_path"] = os.path.normpath(found)
                self.save()

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

        ctk.CTkLabel(self, text="Download hero images automatically when generating a wrapper.",
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


class App(ctk.CTk):
    def __init__(self, config):
        super().__init__()
        self.config = config
        self.title("Ludusavi Wrap")
        self.resizable(False, False)
        self._build()
        self.update_idletasks()
        self._form_height = self.winfo_reqheight()
        self.geometry(f"560x{self._form_height}")

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
        ctk.CTkFrame(self, height=2).pack(fill="x", padx=20, pady=12)

        # ── 1 — Executable ──────────────────────────────────────────────────
        self._heading("1 — Game Executable")
        r1 = ctk.CTkFrame(self, fg_color="transparent")
        r1.pack(fill="x", padx=20, pady=(4, 0))
        self._exe_var = ctk.StringVar()
        ctk.CTkEntry(r1, textvariable=self._exe_var, state="readonly", width=400).pack(side="left", padx=(0, 8))
        ctk.CTkButton(r1, text="Browse", width=80, command=self._browse_exe).pack(side="left")

        # ── 2 — Game Name ───────────────────────────────────────────────────
        self._heading("2 — Ludusavi Game Name")
        r2 = ctk.CTkFrame(self, fg_color="transparent")
        r2.pack(fill="x", padx=20, pady=(4, 0))
        self._name_var = ctk.StringVar()
        ctk.CTkEntry(r2, textvariable=self._name_var, width=330).pack(side="left", padx=(0, 8))
        self._search_btn = ctk.CTkButton(r2, text="Search", width=80, command=self._search)
        self._search_btn.pack(side="left")

        self._results_outer = ctk.CTkFrame(self, fg_color="transparent")
        self._results_outer.pack(fill="x", padx=20)
        self._results_box = ctk.CTkScrollableFrame(self._results_outer, height=80, label_text="")

        # ── 3 — Output ──────────────────────────────────────────────────────
        self._heading("3 — Output")
        r3 = ctk.CTkFrame(self, fg_color="transparent")
        r3.pack(fill="x", padx=20, pady=(4, 0))
        self._folder_var = ctk.StringVar(value=self.config.get("default_output_folder"))
        ctk.CTkEntry(r3, textvariable=self._folder_var, width=400).pack(side="left", padx=(0, 8))
        ctk.CTkButton(r3, text="Browse", width=80, command=self._browse_folder).pack(side="left")

        r4 = ctk.CTkFrame(self, fg_color="transparent")
        r4.pack(fill="x", padx=20, pady=(8, 0))
        ctk.CTkLabel(r4, text="Filename:").pack(side="left", padx=(0, 8))
        self._fname_var = ctk.StringVar()
        ctk.CTkEntry(r4, textvariable=self._fname_var, width=320).pack(side="left")

        # ── Generate ────────────────────────────────────────────────────────
        self._generate_btn = ctk.CTkButton(self, text="Generate Wrapper", height=40,
                                           font=ctk.CTkFont(size=14, weight="bold"),
                                           command=self._generate)
        self._generate_btn.pack(pady=20)

        self._status = ctk.CTkLabel(self, text="", font=ctk.CTkFont(size=13), wraplength=500)
        self._status.pack(padx=20)

        # ── Success panel ────────────────────────────────────────────────────
        self._success_frame = ctk.CTkFrame(
            self, fg_color=("#d6f0dc", "#1b3a24"),
            border_width=1, border_color=("#28a745", "#2ea043"),
        )
        ctk.CTkLabel(self._success_frame, text="✓  Wrapper created",
                     font=ctk.CTkFont(size=14, weight="bold"),
                     text_color=("#155724", "#4caf50")).pack(anchor="w", padx=14, pady=(12, 2))
        self._success_path = ctk.CTkLabel(
            self._success_frame, text="", wraplength=490,
            font=ctk.CTkFont(size=11), text_color=("gray30", "gray70"),
        )
        self._success_path.pack(anchor="w", padx=14, pady=(0, 4))
        self._artwork_status = ctk.CTkLabel(
            self._success_frame, text="", wraplength=490,
            font=ctk.CTkFont(size=11), text_color=("gray40", "gray60"),
        )
        self._artwork_status.pack(anchor="w", padx=14, pady=(0, 10))
        success_btns = ctk.CTkFrame(self._success_frame, fg_color="transparent")
        success_btns.pack(anchor="w", padx=10, pady=(0, 12))
        ctk.CTkButton(success_btns, text="Open Folder", width=110,
                      command=self._open_output_folder).pack(side="left", padx=(0, 8))
        ctk.CTkButton(success_btns, text="New Wrapper", width=110,
                      fg_color="transparent", border_width=1,
                      text_color=("gray10", "gray90"),
                      command=self._clear).pack(side="left")

        # ── Armoury Crate instructions ───────────────────────────────────────
        self._instr_frame = ctk.CTkFrame(self, border_width=1)
        ctk.CTkLabel(self._instr_frame, text="Armoury Crate — Next Steps",
                     font=ctk.CTkFont(size=13, weight="bold")).pack(anchor="w", padx=14, pady=(10, 4))
        ctk.CTkLabel(self._instr_frame, text=ARMOURY_STEPS, justify="left",
                     wraplength=490).pack(anchor="w", padx=14, pady=(0, 12))

    # ── helpers ──────────────────────────────────────────────────────────────

    def _heading(self, text):
        ctk.CTkLabel(self, text=text, font=ctk.CTkFont(size=13, weight="bold")).pack(
            anchor="w", padx=20, pady=(14, 0))

    def _browse_exe(self):
        path = filedialog.askopenfilename(
            parent=self, title="Select game executable",
            filetypes=[("Executable", "*.exe"), ("All files", "*.*")],
        )
        if not path:
            return
        path = os.path.normpath(path)
        self._exe_var.set(path)
        base = os.path.splitext(os.path.basename(path))[0]
        self._fname_var.set(f"{base}.bat")
        if not self._name_var.get():
            self._name_var.set(base.replace("_", " ").replace("-", " ").title())

    def _browse_folder(self):
        folder = filedialog.askdirectory(parent=self, title="Select output folder")
        if folder:
            folder = os.path.normpath(folder)
            self._folder_var.set(folder)
            self.config.set("default_output_folder", folder)

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
        exe    = self._exe_var.get().strip()
        name   = self._name_var.get().strip()
        folder = self._folder_var.get().strip()
        fname  = self._fname_var.get().strip()

        if not exe:
            return self._set_status("Please select a game executable.", ok=False)
        if not name:
            return self._set_status("Please enter a Ludusavi game name.", ok=False)
        if not folder:
            return self._set_status("Please select an output folder.", ok=False)
        if not fname:
            return self._set_status("Please enter a wrapper filename.", ok=False)

        if not fname.lower().endswith(".bat"):
            fname += ".bat"

        os.makedirs(folder, exist_ok=True)
        out_path = os.path.join(folder, fname)

        content = BAT_TEMPLATE.format(
            ludusavi_path=self.config.get("ludusavi_path"),
            game_name=name,
            game_exe=exe,
        )
        with open(out_path, "w", newline="") as f:
            f.write(content)

        self._set_status("", ok=True)
        self._generate_btn.pack_forget()
        self._success_path.configure(text=out_path)

        base = os.path.splitext(fname)[0]
        if self.config.get("steamgriddb_enabled") and self.config.get("steamgriddb_api_key"):
            self._artwork_status.configure(text="Fetching hero image…", text_color=("gray40", "gray60"))
            threading.Thread(target=self._fetch_hero, args=(name, folder, base), daemon=True).start()
        else:
            self._artwork_status.configure(text="")

        self._success_frame.pack(fill="x", padx=20, pady=(16, 0))
        self._instr_frame.pack(fill="x", padx=20, pady=(10, 16))
        self.update_idletasks()
        self.geometry(f"560x{self.winfo_reqheight()}")

    def _fetch_hero(self, game_name, folder, base_name):
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

            img_path = os.path.join(folder, f"{base_name}{ext}")
            with open(img_path, "wb") as f:
                f.write(img_resp.content)

            self.after(0, self._on_artwork_done, img_path)

        except Exception as e:
            self.after(0, self._on_artwork_error, str(e))

    def _on_artwork_done(self, img_path):
        self._artwork_status.configure(
            text=f"✓ Hero image saved: {os.path.basename(img_path)}",
            text_color=("#155724", "#4caf50"),
        )

    def _on_artwork_error(self, msg):
        self._artwork_status.configure(
            text=f"⚠ Artwork: {msg}",
            text_color=("#856404", "#ffc107"),
        )

    def _set_status(self, msg, ok=True):
        self._status.configure(text=msg, text_color="green" if ok else "red")

    def _clear(self):
        self._success_frame.pack_forget()
        self._instr_frame.pack_forget()
        self._exe_var.set("")
        self._name_var.set("")
        self._fname_var.set("")
        for w in self._results_box.winfo_children():
            w.destroy()
        self._results_box.pack_forget()
        self._artwork_status.configure(text="")
        self._set_status("", ok=True)
        self._generate_btn.pack(pady=20)
        self.geometry(f"560x{self._form_height}")

    def _open_output_folder(self):
        folder = self._folder_var.get().strip()
        if folder and os.path.isdir(folder):
            os.startfile(folder)

    def _open_settings(self):
        dlg = SetupDialog(self, self.config, lambda: None)
        self.wait_window(dlg)


def main():
    ctk.set_appearance_mode("system")
    ctk.set_default_color_theme("blue")

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
