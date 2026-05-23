using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Net.Http;
using System.Reflection;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Text.RegularExpressions;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using Microsoft.Win32;

namespace LudusaviWrap
{
    public class LudusaviFindResponse
    {
        [JsonPropertyName("games")]
        public Dictionary<string, object>? Games { get; set; }
    }

    [JsonSourceGenerationOptions(WriteIndented = true)]
    [JsonSerializable(typeof(LudusaviFindResponse))]
    internal partial class MainSourceGenerationContext : JsonSerializerContext
    {
    }

    public partial class MainWindow : Wpf.Ui.Controls.FluentWindow
    {
        public static readonly string Version =
            System.Reflection.Assembly.GetEntryAssembly()?.GetName().Version?.ToString(3) ?? "0.0.0";

        private readonly Config _config;
        private static readonly HttpClient HttpClient = new HttpClient();
        private SuccessWindow? _successWindow;

        public MainWindow(Config config)
        {
            InitializeComponent();
            _config = config;
            Title = $"Ludusavi Wrap v{Version}";

            Loaded += MainWindow_Loaded;
        }

        private void MainWindow_Loaded(object sender, RoutedEventArgs e)
        {
            // AutoUpdater reads the assembly version automatically — no InstalledVersion override needed.
            // The assembly version is stamped from the git tag via /p:Version during dotnet publish.
            // Start() MUST be called from the UI thread: AutoUpdater captures SynchronizationContext.Current
            // to marshal CheckForUpdateEvent back to the UI thread. Task.Run strips that context, which
            // prevents the update dialog from ever appearing.
            AutoUpdaterDotNET.AutoUpdater.ShowSkipButton = false;
            AutoUpdaterDotNET.AutoUpdater.ShowRemindLaterButton = false;
            AutoUpdaterDotNET.AutoUpdater.SetOwner(this);
            AutoUpdaterDotNET.AutoUpdater.CheckForUpdateEvent += (args) =>
            {
                if (args.Error != null) return; // silently ignore network/parse failures
                if (args.IsUpdateAvailable)
                    AutoUpdaterDotNET.AutoUpdater.ShowUpdateForm(args);
            };
            AutoUpdaterDotNET.AutoUpdater.Start(
                "https://raw.githubusercontent.com/aidankinzett/ludusavi-wrap/main/update.xml");
        }

        private void Settings_Click(object sender, RoutedEventArgs e)
        {
            var setup = new SetupWindow(_config);
            setup.Owner = this;
            setup.ShowDialog();
        }

        private void BrowseExe_Click(object sender, RoutedEventArgs e)
        {
            var dialog = new OpenFileDialog
            {
                Title = "Select game executable",
                Filter = "Executable (*.exe)|*.exe|All files (*.*)|*.*",
                RestoreDirectory = true
            };

            if (dialog.ShowDialog() == true)
            {
                ExePathTextBox.Text = dialog.FileName;
                if (string.IsNullOrEmpty(GameNameTextBox.Text.Trim()))
                {
                    string filename = Path.GetFileNameWithoutExtension(dialog.FileName);
                    // Standardize filename to title case name
                    string rawName = filename.Replace("_", " ").Replace("-", " ");
                    GameNameTextBox.Text = System.Globalization.CultureInfo.CurrentCulture.TextInfo.ToTitleCase(rawName);
                }
            }
        }

        private void TextBox_GotFocus(object sender, RoutedEventArgs e)
        {
            ShowTouchKeyboard();
        }

        private void ShowTouchKeyboard()
        {
            try
            {
                foreach (string programFiles in new[] { Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86) })
                {
                    string tabTip = Path.Combine(programFiles, "Common Files", "microsoft shared", "ink", "TabTip.exe");
                    if (File.Exists(tabTip))
                    {
                        Process.Start(tabTip);
                        return;
                    }
                }
            }
            catch
            {
                // Ignore errors
            }
        }

        private async void Search_Click(object sender, RoutedEventArgs e)
        {
            string query = GameNameTextBox.Text.Trim();
            if (string.IsNullOrEmpty(query)) return;

            SearchButton.IsEnabled = false;
            SearchButton.Content = "Searching...";
            ResultsBorder.Visibility = Visibility.Collapsed;

            try
            {
                var games = await SearchGamesInLudusaviAsync(query);
                ShowSearchResults(games);
            }
            catch (Exception ex)
            {
                ShowStatus($"Search failed: {ex.Message}", success: false);
            }
            finally
            {
                SearchButton.IsEnabled = true;
                SearchButton.Content = "Search";
            }
        }

        private async Task<List<string>> SearchGamesInLudusaviAsync(string query)
        {
            if (!_config.IsLudusaviOk)
            {
                throw new FileNotFoundException("Ludusavi executable not found. Please check Settings.");
            }

            string ludusavi = _config.Data.LudusaviPath;

            var process = new Process
            {
                StartInfo = new ProcessStartInfo
                {
                    FileName = ludusavi,
                    Arguments = $"find --api --fuzzy --multiple \"{query}\"",
                    UseShellExecute = false,
                    RedirectStandardOutput = true,
                    CreateNoWindow = true
                }
            };

            process.Start();
            string json = await process.StandardOutput.ReadToEndAsync();
            await process.WaitForExitAsync();
            process.Dispose();

            var response = JsonSerializer.Deserialize(json, MainSourceGenerationContext.Default.LudusaviFindResponse);
            return response?.Games?.Keys.ToList() ?? new List<string>();
        }

        private void ShowSearchResults(List<string> games)
        {
            ResultsListBox.Items.Clear();
            if (games.Count > 0)
            {
                foreach (string game in games.Take(12))
                {
                    ResultsListBox.Items.Add(game);
                }
                ResultsBorder.Visibility = Visibility.Visible;
            }
            else
            {
                ResultsBorder.Visibility = Visibility.Collapsed;
            }
        }

        private void ResultsListBox_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (ResultsListBox.SelectedItem != null)
            {
                GameNameTextBox.Text = ResultsListBox.SelectedItem.ToString();
                ResultsBorder.Visibility = Visibility.Collapsed;
            }
        }

        private void SetButtonsEnabled(bool enabled)
        {
            GenerateArmouryCrateButton.IsEnabled = enabled;
            AddToSteamButton.IsEnabled = enabled;
        }

        // Returns (launcherExePath, safeName) on success, null on validation failure or error
        private async Task<(string exePath, string safeName)?> GenerateLauncherAsync()
        {
            string exe = ExePathTextBox.Text.Trim();
            string name = GameNameTextBox.Text.Trim();

            if (string.IsNullOrEmpty(exe))
            {
                ShowStatus("Please select a game executable.", success: false);
                return null;
            }
            if (string.IsNullOrEmpty(name))
            {
                ShowStatus("Please enter a Ludusavi game name.", success: false);
                return null;
            }
            if (!_config.IsLudusaviOk)
            {
                ShowStatus("Ludusavi not found - open Settings to configure it.", success: false);
                return null;
            }

            string safe = MakeSafeFilename(name);
            if (string.IsNullOrEmpty(safe))
            {
                ShowStatus("Game name contains only invalid filename characters.", success: false);
                return null;
            }

            string appData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            string launchersDir = Path.Combine(appData, "ludusavi-wrap", "launchers");
            string exePath = Path.Combine(launchersDir, safe + ".exe");

            try
            {
                Directory.CreateDirectory(launchersDir);

                var assembly = Assembly.GetExecutingAssembly();
                using (var stream = assembly.GetManifestResourceStream("launcher_stub.exe"))
                {
                    if (stream == null)
                    {
                        ShowStatus("Launcher stub executable resource not found.", success: false);
                        return null;
                    }
                    using (var fileStream = new FileStream(exePath, FileMode.Create, FileAccess.Write))
                    {
                        await stream.CopyToAsync(fileStream);
                    }
                }
            }
            catch (Exception ex)
            {
                ShowStatus($"Failed to copy launcher stub: {ex.Message}", success: false);
                return null;
            }

            string ludusaviWrapExe = _config.Data.LudusaviWrapExe;
            string payload = $"\r\nLUDUSAVI_WRAP_CFG_START\r\n{name}\r\n{exe}\r\n{ludusaviWrapExe}\r\nLUDUSAVI_WRAP_CFG_END\r\n";

            try
            {
                byte[] payloadBytes = Encoding.UTF8.GetBytes(payload);
                using (var fileStream = new FileStream(exePath, FileMode.Append, FileAccess.Write))
                {
                    await fileStream.WriteAsync(payloadBytes, 0, payloadBytes.Length);
                }
            }
            catch (Exception ex)
            {
                try { File.Delete(exePath); } catch { }
                ShowStatus($"Failed to write launcher configuration: {ex.Message}", success: false);
                return null;
            }

            return (exePath, safe);
        }

        private async void GenerateArmouryCrate_Click(object sender, RoutedEventArgs e)
        {
            SetButtonsEnabled(false);
            try
            {
                var result = await GenerateLauncherAsync();
                if (result == null) return;

                var (exePath, safeName) = result.Value;
                string name = GameNameTextBox.Text.Trim();

                ShowStatus("", success: true);
                _successWindow = new SuccessWindow(this, name, exePath, SuccessMode.ArmouryCrate);

                if (_config.Data.SteamGridDbEnabled && !string.IsNullOrEmpty(_config.Data.SteamGridDbApiKey))
                {
                    _successWindow.UpdateArtwork("Fetching cover image...", "#99FFFFFF");
                    _ = Task.Run(() => FetchCoverArtAsync(name, safeName, exePath));
                }

                _successWindow.ShowDialog();
                ClearForm();
            }
            finally
            {
                SetButtonsEnabled(true);
            }
        }

        private async void AddToSteam_Click(object sender, RoutedEventArgs e)
        {
            SetButtonsEnabled(false);
            try
            {
                var result = await GenerateLauncherAsync();
                if (result == null) return;

                var (launcherExePath, safeName) = result.Value;
                string name = GameNameTextBox.Text.Trim();

                string? steamPath = SteamIntegration.GetSteamInstallPath();
                if (steamPath == null)
                {
                    ShowStatus("Steam installation not found. Is Steam installed?", success: false);
                    return;
                }

                var users = SteamIntegration.GetSteamUsers(steamPath);
                if (users.Count == 0)
                {
                    ShowStatus("No Steam user profiles found. Launch Steam at least once to create your profile.", success: false);
                    return;
                }

                var targetUser = users.OrderByDescending(u => u.LastModified).First();

                if (SteamIntegration.IsSteamRunning())
                {
                    var answer = MessageBox.Show(
                        "Steam is currently running. Writing to shortcuts.vdf while Steam is open may cause your changes to be overwritten when Steam exits.\n\nClose Steam first, or the shortcut may not appear.\n\nContinue anyway?",
                        "Steam Is Running",
                        MessageBoxButton.YesNo,
                        MessageBoxImage.Warning);
                    if (answer == MessageBoxResult.No)
                        return;
                }

                VDFParser.Models.VDFEntry[] entries;
                try
                {
                    entries = SteamIntegration.ReadShortcuts(targetUser.ShortcutsPath);
                }
                catch (Exception ex)
                {
                    ShowStatus($"Failed to read shortcuts.vdf: {ex.Message}", success: false);
                    return;
                }

                string startDir = Path.GetDirectoryName(launcherExePath) ?? "";
                SteamIntegration.UpsertShortcut(ref entries, name, launcherExePath, startDir);

                try
                {
                    SteamIntegration.WriteShortcuts(targetUser.ShortcutsPath, entries);
                }
                catch (Exception ex)
                {
                    ShowStatus($"Failed to write shortcuts.vdf: {ex.Message}", success: false);
                    return;
                }

                uint appId = SteamIntegration.CalculateAppId(launcherExePath, name);

                ShowStatus("", success: true);
                string multiUserNote = users.Count > 1 ? $" (Steam user {targetUser.UserId})" : "";
                _successWindow = new SuccessWindow(this, name, launcherExePath, SuccessMode.Steam);

                if (_config.Data.SteamGridDbEnabled && !string.IsNullOrEmpty(_config.Data.SteamGridDbApiKey))
                {
                    _successWindow.UpdateArtwork($"Fetching cover image{multiUserNote}...", "#99FFFFFF");
                    _ = Task.Run(() => FetchCoverArtForSteamAsync(name, safeName, steamPath, targetUser.UserId, appId, multiUserNote));
                }
                else if (users.Count > 1)
                {
                    _successWindow.UpdateArtwork($"Added to Steam user {targetUser.UserId}", "#4CAF50");
                }

                _successWindow.ShowDialog();
                ClearForm();
            }
            finally
            {
                SetButtonsEnabled(true);
            }
        }

        private async Task FetchCoverArtAsync(string gameName, string safeName, string exePath)
        {
            string appData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            string coversDir = Path.Combine(appData, "ludusavi-wrap", "covers");

            try
            {
                var sgdbClient = new SteamGridDbClient(_config.Data.SteamGridDbApiKey);
                var searchResults = await sgdbClient.SearchGameAsync(gameName);
                if (searchResults.Count == 0)
                {
                    Dispatcher.Invoke(() => _successWindow?.UpdateArtwork("⚠ Artwork: Game not found on SteamGridDB", "#FFC107"));
                    return;
                }

                int gameId = searchResults[0].Id;
                string? imgPath = await sgdbClient.DownloadGridImageAsync(gameId, safeName, coversDir);
                if (imgPath == null)
                {
                    Dispatcher.Invoke(() => _successWindow?.UpdateArtwork("⚠ Artwork: No horizontal grid images found on SteamGridDB", "#FFC107"));
                }
                else
                {
                    Dispatcher.Invoke(() => _successWindow?.UpdateArtwork($"Cover art: {imgPath}", "#4CAF50"));
                }
            }
            catch (Exception ex)
            {
                Dispatcher.Invoke(() => _successWindow?.UpdateArtwork($"⚠ Artwork error: {ex.Message}", "#FFC107"));
            }
        }

        private async Task FetchCoverArtForSteamAsync(
            string gameName,
            string safeName,
            string steamPath,
            string userId,
            uint appId,
            string multiUserNote)
        {
            string appData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            string coversDir = Path.Combine(appData, "ludusavi-wrap", "covers");
            string gridDir = Path.Combine(steamPath, "userdata", userId, "config", "grid");

            try
            {
                var sgdb = new SteamGridDbClient(_config.Data.SteamGridDbApiKey);
                var searchResults = await sgdb.SearchGameAsync(gameName);
                if (searchResults.Count == 0)
                {
                    Dispatcher.Invoke(() => _successWindow?.UpdateArtwork("⚠ Artwork: Game not found on SteamGridDB", "#FFC107"));
                    return;
                }

                int gameId = searchResults[0].Id;
                Directory.CreateDirectory(coversDir);
                Directory.CreateDirectory(gridDir);

                // Fetch all four image types in parallel
                string gridBase = Path.Combine(gridDir, appId.ToString());
                var tHorizontal = sgdb.DownloadGridImageAsync(gameId, safeName, coversDir);
                var tPortrait   = sgdb.DownloadPortraitAsync(gameId, gridBase + "p");
                var tHero       = sgdb.DownloadHeroAsync(gameId, gridBase + "_hero");
                var tLogo       = sgdb.DownloadLogoAsync(gameId, gridBase + "_logo");
                await Task.WhenAll(tHorizontal, tPortrait, tHero, tLogo);
                string? horizontal = tHorizontal.Result;
                string? portrait   = tPortrait.Result;
                string? hero       = tHero.Result;
                string? logo       = tLogo.Result;

                // Copy horizontal grid to Steam (portrait/hero/logo already written directly to grid dir)
                SteamIntegration.CopyGridImage(horizontal, steamPath, userId, appId, suffix: "");

                int copied = new[] { horizontal, portrait, hero, logo }.Count(p => p != null);
                if (copied == 0)
                {
                    Dispatcher.Invoke(() => _successWindow?.UpdateArtwork(
                        $"⚠ Artwork: No images found on SteamGridDB{multiUserNote}", "#FFC107"));
                }
                else
                {
                    string detail = string.Join(", ", new[]
                    {
                        horizontal != null ? "grid" : null,
                        portrait  != null ? "portrait" : null,
                        hero      != null ? "hero" : null,
                        logo      != null ? "logo" : null,
                    }.Where(s => s != null));
                    Dispatcher.Invoke(() => _successWindow?.UpdateArtwork(
                        $"Art added to Steam ({detail}){multiUserNote}", "#4CAF50"));
                }
            }
            catch (Exception ex)
            {
                Dispatcher.Invoke(() => _successWindow?.UpdateArtwork($"⚠ Artwork error: {ex.Message}", "#FFC107"));
            }
        }

        private static string MakeSafeFilename(string name)
        {
            string invalidChars = Regex.Escape(new string(Path.GetInvalidFileNameChars()));
            string invalidRegStr = string.Format(@"([{0}]*\.+$)|([{0}]+)", invalidChars);
            return Regex.Replace(name, invalidRegStr, "_").Trim();
        }

        private void ShowStatus(string message, bool success)
        {
            StatusLabel.Text = message;
            StatusLabel.Foreground = success ? Brushes.Green : Brushes.Red;
            StatusLabel.Visibility = string.IsNullOrEmpty(message) ? Visibility.Collapsed : Visibility.Visible;
        }

        private void ClearForm()
        {
            ExePathTextBox.Text = "";
            GameNameTextBox.Text = "";
            ResultsListBox.Items.Clear();
            ResultsBorder.Visibility = Visibility.Collapsed;
            ShowStatus("", success: true);
        }
    }
}
