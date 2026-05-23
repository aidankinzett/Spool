import customtkinter as ctk
from tkinter import filedialog
import json
import os
import subprocess
import threading

CONFIG_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "config.json")

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
    "6.  Set the game title and add cover art"
)


class Config:
    def __init__(self):
        self.data = {"ludusavi_path": "", "default_output_folder": ""}
        self._load()

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
        self.title("Setup")
        self.geometry("500x190")
        self.resizable(False, False)
        self.grab_set()

        ctk.CTkLabel(self, text="Where is ludusavi.exe?",
                     font=ctk.CTkFont(size=15, weight="bold")).pack(pady=(24, 6))

        row = ctk.CTkFrame(self, fg_color="transparent")
        row.pack(fill="x", padx=24)
        self._path_var = ctk.StringVar(value=config.get("ludusavi_path"))
        ctk.CTkEntry(row, textvariable=self._path_var, width=360).pack(side="left", padx=(0, 8))
        ctk.CTkButton(row, text="Browse", width=80, command=self._browse).pack(side="left")

        self._err = ctk.CTkLabel(self, text="", text_color="red", font=ctk.CTkFont(size=12))
        self._err.pack(pady=4)

        ctk.CTkButton(self, text="Save & Continue", command=self._save).pack(pady=(0, 20))

    def _browse(self):
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
        self.config.set("ludusavi_path", path)
        self.destroy()
        self.on_saved()


class App(ctk.CTk):
    def __init__(self, config):
        super().__init__()
        self.config = config
        self.title("Ludusavi Wrap")
        self.geometry("560x640")
        self.resizable(False, False)
        self._build()

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
        self._results_box = ctk.CTkScrollableFrame(self._results_outer, height=80,
                                                    label_text="")

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
        ctk.CTkButton(self, text="Generate Wrapper", height=40,
                      font=ctk.CTkFont(size=14, weight="bold"),
                      command=self._generate).pack(pady=20)

        self._status = ctk.CTkLabel(self, text="", font=ctk.CTkFont(size=13), wraplength=500)
        self._status.pack(padx=20)

        # ── Armoury Crate instructions (hidden until generation succeeds) ───
        self._instr_frame = ctk.CTkFrame(self, border_width=1)
        ctk.CTkLabel(self._instr_frame, text="Armoury Crate — Next Steps",
                     font=ctk.CTkFont(size=13, weight="bold")).pack(anchor="w", padx=14, pady=(10, 4))
        ctk.CTkLabel(self._instr_frame, text=ARMOURY_STEPS, justify="left",
                     wraplength=490).pack(anchor="w", padx=14, pady=(0, 12))

    # ── helpers ─────────────────────────────────────────────────────────────

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

        self._set_status(f"✓ Created: {out_path}", ok=True)
        self._instr_frame.pack(fill="x", padx=20, pady=(10, 16))

    def _set_status(self, msg, ok=True):
        self._status.configure(text=msg, text_color="green" if ok else "red")

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
