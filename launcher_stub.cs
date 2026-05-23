using System;
using System.IO;
using System.Text;
using System.Diagnostics;
using System.Windows.Forms;

class Launcher {
    [STAThread]
    static void Main() {
        try {
            string exePath = Process.GetCurrentProcess().MainModule.FileName;
            byte[] fileBytes;
            using (var fs = new FileStream(exePath, FileMode.Open, FileAccess.Read, FileShare.ReadWrite)) {
                fileBytes = new byte[fs.Length];
                fs.Read(fileBytes, 0, fileBytes.Length);
            }
            
            string fileStr = Encoding.UTF8.GetString(fileBytes);
            
            string startMarker = "LUDUSAVI_WRAP_CFG_START\n";
            string endMarker = "\nLUDUSAVI_WRAP_CFG_END";
            
            int startIdx = fileStr.IndexOf(startMarker);
            if (startIdx == -1) {
                startMarker = "LUDUSAVI_WRAP_CFG_START\r\n";
                endMarker = "\r\nLUDUSAVI_WRAP_CFG_END";
                startIdx = fileStr.IndexOf(startMarker);
            }
            
            if (startIdx == -1) {
                MessageBox.Show("Configuration not found in launcher executable.", "Ludusavi Wrap Error", MessageBoxButtons.OK, MessageBoxIcon.Error);
                return;
            }
            
            int endIdx = fileStr.IndexOf(endMarker, startIdx);
            if (endIdx == -1) {
                MessageBox.Show("Configuration end marker not found.", "Ludusavi Wrap Error", MessageBoxButtons.OK, MessageBoxIcon.Error);
                return;
            }
            
            string configData = fileStr.Substring(startIdx + startMarker.Length, endIdx - (startIdx + startMarker.Length));
            string[] lines = configData.Split(new[] { "\r\n", "\n" }, StringSplitOptions.None);
            if (lines.Length < 3) {
                MessageBox.Show("Invalid launcher configuration format.", "Ludusavi Wrap Error", MessageBoxButtons.OK, MessageBoxIcon.Error);
                return;
            }
            
            string gameName = lines[0].Trim();
            string gameExe = lines[1].Trim();
            string fallbackWrapExe = lines[2].Trim();
            
            // Try to find the current path of ludusavi-wrap from global config
            string wrapExe = fallbackWrapExe;
            string wrapArgs = "";
            GetWrapExeFromGlobalConfig(ref wrapExe, ref wrapArgs);
            
            if (!File.Exists(wrapExe)) {
                MessageBox.Show("Ludusavi Wrap main executable not found at:\n" + wrapExe + "\n\nPlease run Ludusavi Wrap to update the configuration.", "Ludusavi Wrap Error", MessageBoxButtons.OK, MessageBoxIcon.Error);
                return;
            }
            
            string arguments;
            if (!string.IsNullOrEmpty(wrapArgs)) {
                arguments = string.Format("\"{0}\" --run \"{1}\" \"{2}\"", wrapArgs, gameName, gameExe);
            } else {
                arguments = string.Format("--run \"{0}\" \"{1}\"", gameName, gameExe);
            }
            
            var startInfo = new ProcessStartInfo {
                FileName = wrapExe,
                Arguments = arguments,
                UseShellExecute = false,
                CreateNoWindow = true
            };
            
            using (var proc = Process.Start(startInfo)) {
                if (proc != null) {
                    proc.WaitForExit();
                } else {
                    MessageBox.Show("Failed to launch ludusavi-wrap process.", "Ludusavi Wrap Error", MessageBoxButtons.OK, MessageBoxIcon.Error);
                }
            }
        } catch (Exception ex) {
            MessageBox.Show("An unexpected error occurred in the launcher shortcut: " + ex.Message, "Ludusavi Wrap Error", MessageBoxButtons.OK, MessageBoxIcon.Error);
        }
    }
    
    static void GetWrapExeFromGlobalConfig(ref string wrapExe, ref string wrapArgs) {
        try {
            string appData = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
            string configPath = Path.Combine(appData, "ludusavi-wrap", "config.json");
            if (File.Exists(configPath)) {
                string json = File.ReadAllText(configPath);
                
                string exeKey = "\"ludusavi_wrap_exe\":";
                int exeIdx = json.IndexOf(exeKey);
                if (exeIdx != -1) {
                    int startQuote = json.IndexOf("\"", exeIdx + exeKey.Length);
                    if (startQuote != -1) {
                        int endQuote = json.IndexOf("\"", startQuote + 1);
                        if (endQuote != -1) {
                            string path = json.Substring(startQuote + 1, endQuote - startQuote - 1);
                            path = path.Replace("\\\\", "\\");
                            if (File.Exists(path)) {
                                wrapExe = path;
                            }
                        }
                    }
                }
                
                string argsKey = "\"ludusavi_wrap_args\":";
                int argsIdx = json.IndexOf(argsKey);
                if (argsIdx != -1) {
                    int startQuote = json.IndexOf("\"", argsIdx + argsKey.Length);
                    if (startQuote != -1) {
                        int endQuote = json.IndexOf("\"", startQuote + 1);
                        if (endQuote != -1) {
                            string args = json.Substring(startQuote + 1, endQuote - startQuote - 1);
                            args = args.Replace("\\\\", "\\");
                            wrapArgs = args;
                        }
                    }
                }
            }
        } catch {
            // Ignore errors and fall back to hardcoded parameters
        }
    }
}
