using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using Microsoft.Win32;

namespace LudusaviWrap
{
    public partial class AddGameWindow : Wpf.Ui.Controls.FluentWindow
    {
        private readonly Config _config;
        private readonly GameLibrary _library;
        private SuccessWindow? _successWindow;

        // Invoked on the UI thread when async cover art download completes after the dialog closes.
        public Action<GameEntry, string>? OnCoverArtFetched { get; set; }

        public AddGameWindow(Config config, GameLibrary library)
        {
            InitializeComponent();
            _config = config;
            _library = library;
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
                RunAsAdminCheckBox.IsChecked = RegistryHelper.GetCompatFlagRunAsAdmin(dialog.FileName);
                if (string.IsNullOrEmpty(GameNameTextBox.Text.Trim()))
                {
                    string filename = Path.GetFileNameWithoutExtension(dialog.FileName);
                    string rawName = filename.Replace("_", " ").Replace("-", " ");
                    GameNameTextBox.Text = System.Globalization.CultureInfo.CurrentCulture.TextInfo.ToTitleCase(rawName);
                }
                // Auto-populate game folder from exe directory if not already set
                if (string.IsNullOrEmpty(FolderPathTextBox.Text))
                    FolderPathTextBox.Text = Path.GetDirectoryName(dialog.FileName) ?? "";
            }
        }

        private void BrowseFolder_Click(object sender, RoutedEventArgs e)
        {
            var dialog = new OpenFolderDialog
            {
                Title = "Select the root folder of the game installation",
                Multiselect = false
            };
            if (dialog.ShowDialog() == true)
                FolderPathTextBox.Text = dialog.FolderName;
        }

        private void TextBox_GotFocus(object sender, RoutedEventArgs e) => ShowTouchKeyboard();

        private void ShowTouchKeyboard()
        {
            try
            {
                foreach (string programFiles in new[] {
                    Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles),
                    Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86) })
                {
                    string tabTip = Path.Combine(programFiles, "Common Files", "microsoft shared", "ink", "TabTip.exe");
                    if (File.Exists(tabTip))
                    {
                        Process.Start(tabTip);
                        return;
                    }
                }
            }
            catch { }
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
                throw new FileNotFoundException("Ludusavi executable not found. Please check Settings.");

            var process = new Process
            {
                StartInfo = new ProcessStartInfo
                {
                    FileName = _config.Data.LudusaviPath,
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
                    ResultsListBox.Items.Add(game);
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

        private (string exe, string name, string safe)? ValidateFields()
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

            string safe = LauncherGenerator.MakeSafeFilename(name);
            if (string.IsNullOrEmpty(safe))
            {
                ShowStatus("Game name contains only invalid filename characters.", success: false);
                return null;
            }

            return (exe, name, safe);
        }

        private void ApplyRunAsAdmin(GameEntry entry)
        {
            entry.RunAsAdmin = RunAsAdminCheckBox.IsChecked == true;
            if (entry.RunAsAdmin)
            {
                RegistryHelper.SetCompatFlagRunAsAdmin(entry.ExePath);
            }
            else
            {
                RegistryHelper.RemoveCompatFlagRunAsAdmin(entry.ExePath);
            }
        }

        private void AddToLibrary_Click(object sender, RoutedEventArgs e)
        {
            var fields = ValidateFields();
            if (fields == null) return;
            var (exe, name, safe) = fields.Value;

            SetButtonsEnabled(false);
            try
            {
                var existing = _library.FindByName(name);
                if (existing != null)
                {
                    var ans = MessageBox.Show($"'{name}' is already in your library. Update it?",
                        "Already Exists", MessageBoxButton.YesNo, MessageBoxImage.Question);
                    if (ans != MessageBoxResult.Yes) return;
                    existing.ExePath = exe;
                    existing.SafeName = safe;
                    ApplyRunAsAdmin(existing);
                    _library.Update(existing);
                    DialogResult = true;
                    Close();
                    return;
                }

                string folder = FolderPathTextBox.Text.Trim();
                var entry = new GameEntry
                {
                    GameName       = name,
                    ExePath        = exe,
                    SafeName       = safe,
                    GameFolderPath = folder.Length > 0 ? folder : null,
                    AddedAt        = DateTime.UtcNow
                };
                ApplyRunAsAdmin(entry);
                _library.Add(entry);

                if (_config.Data.SteamGridDbEnabled && !string.IsNullOrEmpty(_config.Data.SteamGridDbApiKey))
                    _ = Task.Run(() => FetchAndUpdateCoverArtAsync(entry));

                DialogResult = true;
                Close();
            }
            finally
            {
                SetButtonsEnabled(true);
            }
        }

        private async Task FetchAndUpdateCoverArtAsync(GameEntry entry)
        {
            try
            {
                string appData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
                string coversDir = Path.Combine(appData, "ludusavi-wrap", "covers");
                Directory.CreateDirectory(coversDir);

                var sgdb = new SteamGridDbClient(_config.Data.SteamGridDbApiKey);
                var results = await sgdb.SearchGameAsync(entry.GameName);
                if (results.Count == 0) return;

                int gameId = results[0].Id;
                // Portrait preferred for library cards, fall back to horizontal grid
                string destBase = Path.Combine(coversDir, entry.SafeName);
                string? imagePath = await sgdb.DownloadPortraitAsync(gameId, destBase + "_p");
                imagePath ??= await sgdb.DownloadGridImageAsync(gameId, entry.SafeName, coversDir);

                if (imagePath != null)
                {
                    Application.Current?.Dispatcher.Invoke(() =>
                    {
                        entry.CoverImagePath = imagePath;
                        _library.Update(entry);
                        OnCoverArtFetched?.Invoke(entry, imagePath);
                    });
                }
            }
            catch (Exception ex)
            {
                App.Log($"[AddGameWindow] Failed to fetch cover art for '{entry.GameName}': {ex.Message}");
            }
        }

        private async void GenerateArmouryCrate_Click(object sender, RoutedEventArgs e)
        {
            var fields = ValidateFields();
            if (fields == null) return;
            var (exe, name, safe) = fields.Value;

            SetButtonsEnabled(false);
            try
            {
                GameEntry entry;
                string launcherExePath;
                try
                {
                    string folder = FolderPathTextBox.Text.Trim();
                    var existing = _library.FindByName(name);
                    entry = existing ?? new GameEntry { GameName = name, ExePath = exe, SafeName = safe, GameFolderPath = folder.Length > 0 ? folder : null, AddedAt = DateTime.UtcNow };
                    if (existing != null && string.IsNullOrEmpty(existing.GameFolderPath) && folder.Length > 0)
                        entry.GameFolderPath = folder;
                    ApplyRunAsAdmin(entry);
                    launcherExePath = await LauncherGenerator.GenerateLauncherExeAsync(entry, _config);
                    entry.LauncherExePath = launcherExePath;
                    if (existing == null) _library.Add(entry); else _library.Update(entry);
                }
                catch (Exception ex)
                {
                    ShowStatus($"Failed to generate launcher: {ex.Message}", success: false);
                    return;
                }

                _successWindow = new SuccessWindow(this, name, launcherExePath, SuccessMode.ArmouryCrate);

                if (_config.Data.SteamGridDbEnabled && !string.IsNullOrEmpty(_config.Data.SteamGridDbApiKey))
                {
                    _successWindow.UpdateArtwork("Fetching cover image...", "#99FFFFFF");
                    _ = Task.Run(() => FetchCoverArtForACAsync(name, safe));
                }

                _successWindow.ShowDialog();
                DialogResult = true;
                Close();
            }
            finally
            {
                SetButtonsEnabled(true);
            }
        }

        private async Task FetchCoverArtForACAsync(string gameName, string safeName)
        {
            string appData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            string coversDir = Path.Combine(appData, "ludusavi-wrap", "covers");

            try
            {
                var sgdb = new SteamGridDbClient(_config.Data.SteamGridDbApiKey);
                var results = await sgdb.SearchGameAsync(gameName);
                if (results.Count == 0)
                {
                    Dispatcher.Invoke(() => _successWindow?.UpdateArtwork("⚠ Artwork: Game not found on SteamGridDB", "#FFC107"));
                    return;
                }

                int gameId = results[0].Id;
                string? imgPath = await sgdb.DownloadGridImageAsync(gameId, safeName, coversDir);
                if (imgPath == null)
                    Dispatcher.Invoke(() => _successWindow?.UpdateArtwork("⚠ Artwork: No horizontal grid images found on SteamGridDB", "#FFC107"));
                else
                    Dispatcher.Invoke(() => _successWindow?.UpdateArtwork($"Cover art: {imgPath}", "#4CAF50"));
            }
            catch (Exception ex)
            {
                App.Log($"[AddGameWindow] Failed to fetch cover art for Armoury Crate '{gameName}': {ex.Message}");
                Dispatcher.Invoke(() => _successWindow?.UpdateArtwork($"⚠ Artwork error: {ex.Message}", "#FFC107"));
            }
        }

        private async void AddToSteam_Click(object sender, RoutedEventArgs e)
        {
            var fields = ValidateFields();
            if (fields == null) return;
            var (exe, name, safe) = fields.Value;

            SetButtonsEnabled(false);
            try
            {
                GameEntry entry;
                string launcherExePath;
                try
                {
                    string folder = FolderPathTextBox.Text.Trim();
                    var existing = _library.FindByName(name);
                    entry = existing ?? new GameEntry { GameName = name, ExePath = exe, SafeName = safe, GameFolderPath = folder.Length > 0 ? folder : null, AddedAt = DateTime.UtcNow };
                    if (existing != null && string.IsNullOrEmpty(existing.GameFolderPath) && folder.Length > 0)
                        entry.GameFolderPath = folder;
                    ApplyRunAsAdmin(entry);
                    launcherExePath = await LauncherGenerator.GenerateLauncherExeAsync(entry, _config);
                    entry.LauncherExePath = launcherExePath;
                    if (existing == null) _library.Add(entry); else _library.Update(entry);
                }
                catch (Exception ex)
                {
                    ShowStatus($"Failed to generate launcher: {ex.Message}", success: false);
                    return;
                }

                string? steamPath = await Task.Run(() => SteamIntegration.GetSteamInstallPath());
                if (steamPath == null)
                {
                    ShowStatus("Steam installation not found. Is Steam installed?", success: false);
                    return;
                }

                var users = await Task.Run(() => SteamIntegration.GetSteamUsers(steamPath));
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
                        "Steam Is Running", MessageBoxButton.YesNo, MessageBoxImage.Warning);
                    if (answer == MessageBoxResult.No) return;
                }

                VDFParser.Models.VDFEntry[] entries;
                try
                {
                    entries = await Task.Run(() => SteamIntegration.ReadShortcuts(targetUser.ShortcutsPath));
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
                    await Task.Run(() => SteamIntegration.WriteShortcuts(targetUser.ShortcutsPath, entries));
                }
                catch (Exception ex)
                {
                    ShowStatus($"Failed to write shortcuts.vdf: {ex.Message}", success: false);
                    return;
                }

                uint appId = SteamIntegration.CalculateAppId(launcherExePath, name);
                string multiUserNote = users.Count > 1 ? $" (Steam user {targetUser.UserId})" : "";
                _successWindow = new SuccessWindow(this, name, launcherExePath, SuccessMode.Steam);

                if (_config.Data.SteamGridDbEnabled && !string.IsNullOrEmpty(_config.Data.SteamGridDbApiKey))
                {
                    _successWindow.UpdateArtwork($"Fetching cover image{multiUserNote}...", "#99FFFFFF");
                    _ = Task.Run(() => FetchCoverArtForSteamAsync(name, safe, steamPath, targetUser.UserId, appId, multiUserNote));
                }
                else if (users.Count > 1)
                {
                    _successWindow.UpdateArtwork($"Added to Steam user {targetUser.UserId}", "#4CAF50");
                }

                _successWindow.ShowDialog();
                DialogResult = true;
                Close();
            }
            finally
            {
                SetButtonsEnabled(true);
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
                var results = await sgdb.SearchGameAsync(gameName);
                if (results.Count == 0)
                {
                    Dispatcher.Invoke(() => _successWindow?.UpdateArtwork("⚠ Artwork: Game not found on SteamGridDB", "#FFC107"));
                    return;
                }

                int gameId = results[0].Id;
                Directory.CreateDirectory(coversDir);
                Directory.CreateDirectory(gridDir);

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
                        portrait   != null ? "portrait" : null,
                        hero       != null ? "hero" : null,
                        logo       != null ? "logo" : null,
                    }.Where(s => s != null));
                    Dispatcher.Invoke(() => _successWindow?.UpdateArtwork(
                        $"Art added to Steam ({detail}){multiUserNote}", "#4CAF50"));
                }
            }
            catch (Exception ex)
            {
                App.Log($"[AddGameWindow] Failed to fetch cover art for Steam '{gameName}': {ex.Message}");
                Dispatcher.Invoke(() => _successWindow?.UpdateArtwork($"⚠ Artwork error: {ex.Message}", "#FFC107"));
            }
        }

        private void SetButtonsEnabled(bool enabled)
        {
            AddToLibraryButton.IsEnabled = enabled;
            GenerateArmouryCrateButton.IsEnabled = enabled;
            AddToSteamButton.IsEnabled = enabled;
        }

        private void ShowStatus(string message, bool success)
        {
            StatusLabel.Text = message;
            StatusLabel.Foreground = success ? Brushes.Green : Brushes.Red;
            StatusLabel.Visibility = string.IsNullOrEmpty(message) ? Visibility.Collapsed : Visibility.Visible;
        }
    }
}
