using System;
using System.IO;
using System.Reflection;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading.Tasks;

namespace LudusaviWrap
{
    public static class LauncherGenerator
    {
        private static readonly string LaunchersDir = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "ludusavi-wrap", "launchers");

        public static string MakeSafeFilename(string name)
        {
            string invalidChars = Regex.Escape(new string(Path.GetInvalidFileNameChars()));
            string invalidRegStr = string.Format(@"([{0}]*\.+$)|([{0}]+)", invalidChars);
            return Regex.Replace(name, invalidRegStr, "_").Trim();
        }

        // Generates the launcher stub .exe for the given entry. Returns the launcher exe path.
        // Throws on failure.
        public static async Task<string> GenerateLauncherExeAsync(GameEntry entry, Config config)
        {
            string safe = string.IsNullOrEmpty(entry.SafeName)
                ? MakeSafeFilename(entry.GameName)
                : entry.SafeName;
            string exePath = Path.Combine(LaunchersDir, safe + ".exe");

            Directory.CreateDirectory(LaunchersDir);

            var assembly = Assembly.GetExecutingAssembly();
            using (var stream = assembly.GetManifestResourceStream("launcher_stub.exe"))
            {
                if (stream == null)
                    throw new InvalidOperationException("Launcher stub executable resource not found.");
                using var fileStream = new FileStream(exePath, FileMode.Create, FileAccess.Write);
                await stream.CopyToAsync(fileStream);
            }

            string ludusaviWrapExe = config.Data.LudusaviWrapExe;
            string payload = $"\r\nLUDUSAVI_WRAP_CFG_START\r\n{entry.GameName}\r\n{entry.ExePath}\r\n{ludusaviWrapExe}\r\nLUDUSAVI_WRAP_CFG_END\r\n";

            try
            {
                byte[] payloadBytes = Encoding.UTF8.GetBytes(payload);
                using var fileStream = new FileStream(exePath, FileMode.Append, FileAccess.Write);
                await fileStream.WriteAsync(payloadBytes, 0, payloadBytes.Length);
            }
            catch
            {
                try { File.Delete(exePath); } catch { }
                throw;
            }

            return exePath;
        }
    }
}
