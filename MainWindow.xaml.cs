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

    public partial class MainWindow : Window
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

        private async void Generate_Click(object sender, RoutedEventArgs e)
        {
            string exe = ExePathTextBox.Text.Trim();
            string name = GameNameTextBox.Text.Trim();

            if (string.IsNullOrEmpty(exe))
            {
                ShowStatus("Please select a game executable.", success: false);
                return;
            }
            if (string.IsNullOrEmpty(name))
            {
                ShowStatus("Please enter a Ludusavi game name.", success: false);
                return;
            }
            if (!_config.IsLudusaviOk)
            {
                ShowStatus("Ludusavi not found - open Settings to configure it.", success: false);
                return;
            }

            string safe = MakeSafeFilename(name);
            if (string.IsNullOrEmpty(safe))
            {
                ShowStatus("Game name contains only invalid filename characters.", success: false);
                return;
            }

            string appData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            string launchersDir = Path.Combine(appData, "ludusavi-wrap", "launchers");
            string exePath = Path.Combine(launchersDir, safe + ".exe");

            try
            {
                Directory.CreateDirectory(launchersDir);

                // Write the embedded launcher_stub.exe
                var assembly = Assembly.GetExecutingAssembly();
                using (var stream = assembly.GetManifestResourceStream("launcher_stub.exe"))
                {
                    if (stream == null)
                    {
                        ShowStatus("Launcher stub executable resource not found.", success: false);
                        return;
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
                return;
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
                return;
            }

            ShowStatus("", success: true);
            _successWindow = new SuccessWindow(this, name, exePath);

            // Fetch SteamGridDB Art in background if enabled
            if (_config.Data.SteamGridDbEnabled && !string.IsNullOrEmpty(_config.Data.SteamGridDbApiKey))
            {
                _successWindow.UpdateArtwork("Fetching cover image...", "#99FFFFFF");
                _ = Task.Run(() => FetchCoverArtAsync(name, safe, exePath));
            }

            _successWindow.ShowDialog();
            ClearForm();
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
