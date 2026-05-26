using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Data;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Microsoft.Toolkit.Uwp.Notifications;

namespace LudusaviWrap
{
    public class LudusaviFindResponse
    {
        [JsonPropertyName("games")]
        public Dictionary<string, object>? Games { get; set; }
    }

    [JsonSourceGenerationOptions(WriteIndented = true)]
    [JsonSerializable(typeof(LudusaviFindResponse))]
    internal partial class MainSourceGenerationContext : JsonSerializerContext { }


    // ── MainWindow ──────────────────────────────────────────────────────────────

    public partial class MainWindow : Wpf.Ui.Controls.FluentWindow
    {
        public static readonly string Version =
            System.Reflection.Assembly.GetEntryAssembly()?.GetName().Version?.ToString(3) ?? "0.0.0";

        // ── DependencyProperties ────────────────────────────────────────────────

        public static readonly DependencyProperty IsLanScanningProperty =
            DependencyProperty.Register(nameof(IsLanScanning), typeof(bool), typeof(MainWindow), new PropertyMetadata(false));
        public bool IsLanScanning
        {
            get => (bool)GetValue(IsLanScanningProperty);
            set => SetValue(IsLanScanningProperty, value);
        }

        public static readonly DependencyProperty LanPeersCountProperty =
            DependencyProperty.Register(nameof(LanPeersCount), typeof(int), typeof(MainWindow), new PropertyMetadata(0));
        public int LanPeersCount
        {
            get => (int)GetValue(LanPeersCountProperty);
            set => SetValue(LanPeersCountProperty, value);
        }

        public static readonly DependencyProperty SelectedGameProperty =
            DependencyProperty.Register(nameof(SelectedGame), typeof(GameEntry), typeof(MainWindow), new PropertyMetadata(null));
        public GameEntry? SelectedGame
        {
            get => (GameEntry?)GetValue(SelectedGameProperty);
            set => SetValue(SelectedGameProperty, value);
        }

        public static readonly DependencyProperty HasNoFilteredGamesProperty =
            DependencyProperty.Register(nameof(HasNoFilteredGames), typeof(bool), typeof(MainWindow), new PropertyMetadata(false));
        public bool HasNoFilteredGames
        {
            get => (bool)GetValue(HasNoFilteredGamesProperty);
            set => SetValue(HasNoFilteredGamesProperty, value);
        }

        public static readonly DependencyProperty LibraryCountProperty =
            DependencyProperty.Register(nameof(LibraryCount), typeof(int), typeof(MainWindow), new PropertyMetadata(0));
        public int LibraryCount
        {
            get => (int)GetValue(LibraryCountProperty);
            set => SetValue(LibraryCountProperty, value);
        }

        public static readonly DependencyProperty TotalPlaytimeProperty =
            DependencyProperty.Register(nameof(TotalPlaytime), typeof(string), typeof(MainWindow), new PropertyMetadata("—"));
        public string TotalPlaytime
        {
            get => (string)GetValue(TotalPlaytimeProperty);
            set => SetValue(TotalPlaytimeProperty, value);
        }

        public static readonly DependencyProperty TotalBackupsProperty =
            DependencyProperty.Register(nameof(TotalBackups), typeof(int), typeof(MainWindow), new PropertyMetadata(0));
        public int TotalBackups
        {
            get => (int)GetValue(TotalBackupsProperty);
            set => SetValue(TotalBackupsProperty, value);
        }

        public static readonly DependencyProperty TotalInstallSizeProperty =
            DependencyProperty.Register(nameof(TotalInstallSize), typeof(string), typeof(MainWindow), new PropertyMetadata("—"));
        public string TotalInstallSize
        {
            get => (string)GetValue(TotalInstallSizeProperty);
            set => SetValue(TotalInstallSizeProperty, value);
        }

        // ── Fields ──────────────────────────────────────────────────────────────

        private readonly Config _config;
        private readonly GameLibrary _library;
        private readonly ObservableCollection<GameEntry> _games = new();
        private readonly ObservableCollection<GameEntry> _filteredGames = new();
        private SuccessWindow? _successWindow;
        private bool _updateCheckSubscribed = false;
        private readonly LanShareServer _lanServer;
        private readonly LanShareClient _lanClient;
        private CancellationTokenSource? _scanCts;
        private CancellationTokenSource? _uiCleanupCts;
        private int _scanInProgress = 0;
        private string _activeFilter = "all";
        private string _sortOrder = "recent";
        private string _searchQuery = "";
        private bool _updatingDetail = false;

        // ── Constructor ─────────────────────────────────────────────────────────

        public MainWindow(Config config, GameLibrary library)
        {
            InitializeComponent();
            _config = config;
            _library = library;
            Title = $"Spool v{Version}";

            _lanServer = new LanShareServer(config.Data.DeviceName, config.Data.DeviceId);
            _lanServer.UploadsChanged += LanServer_UploadsChanged;
            _lanServer.PeerActivityDetected += LanServer_PeerActivityDetected;
            _lanClient = new LanShareClient(config.Data.DeviceName, config.Data.DeviceId);

            GameListBox.ItemsSource = _filteredGames;
            LoadGames();
            Loaded  += MainWindow_Loaded;
            Closing += MainWindow_Closing;
        }

        // ── Library management ──────────────────────────────────────────────────

        private void LoadGames()
        {
            var lanCards = _games.Where(g => g.IsLanCard).ToList();
            _games.Clear();
            foreach (var entry in _library.Entries)
                _games.Add(entry);
            foreach (var lanCard in lanCards)
                _games.Add(lanCard);
            ApplyFilterSort();
        }

        private void ApplyFilterSort()
        {
            _updatingDetail = true;
            var prevId = SelectedGame?.Id;

            var local = _library.Entries.AsEnumerable();

            if (!string.IsNullOrEmpty(_searchQuery))
                local = local.Where(e => e.GameName.Contains(_searchQuery, StringComparison.OrdinalIgnoreCase));

            switch (_activeFilter)
            {
                case "recent":
                    local = local.Where(e => e.LastPlayedAt.HasValue);
                    break;
                case "shared":
                    local = local.Where(e => e.LanShared);
                    break;
                case "unplayed":
                    local = local.Where(e => e.PlaytimeMinutes == 0 && !e.LastPlayedAt.HasValue);
                    break;
            }

            local = _sortOrder switch
            {
                "name"     => local.OrderBy(e => e.GameName, StringComparer.OrdinalIgnoreCase),
                "added"    => local.OrderByDescending(e => e.AddedAt),
                "playtime" => local.OrderByDescending(e => e.PlaytimeMinutes),
                "size"     => local.OrderByDescending(e => e.InstallSizeMb),
                _          => local.OrderByDescending(e => e.LastPlayedAt ?? DateTime.MinValue),
            };

            var lanCards = _games.Where(g => g.IsLanCard).AsEnumerable();
            if (!string.IsNullOrEmpty(_searchQuery))
                lanCards = lanCards.Where(g => g.GameName.Contains(_searchQuery, StringComparison.OrdinalIgnoreCase));
            if (_activeFilter != "all" && _activeFilter != "shared")
                lanCards = Enumerable.Empty<GameEntry>();

            _filteredGames.Clear();
            foreach (var e in local)    _filteredGames.Add(e);
            foreach (var e in lanCards) _filteredGames.Add(e);

            if (prevId != null)
            {
                var match = _filteredGames.FirstOrDefault(g => g.Id == prevId);
                GameListBox.SelectedItem = match;
                if (match == null) SelectedGame = null;
            }

            Dispatcher.BeginInvoke(new Action(() => _updatingDetail = false));

            HasNoFilteredGames = _filteredGames.Count == 0;
            UpdateOverviewStats();
        }

        private void UpdateOverviewStats()
        {
            LibraryCount = _library.Entries.Count;
            int totalMin = _library.Entries.Sum(e => e.PlaytimeMinutes);
            int h = totalMin / 60, m = totalMin % 60;
            TotalPlaytime = totalMin <= 0 ? "—" : (m == 0 ? $"{h} h" : $"{h} h {m} min");
            TotalBackups = _library.Entries.Sum(e => e.SaveBackupCount);
            double totalMb = _library.Entries.Sum(e => e.InstallSizeMb);
            TotalInstallSize = totalMb <= 0 ? "—"
                : totalMb < 1024 ? $"{totalMb:0.0} MB"
                : $"{totalMb / 1024:0.0} GB";
        }

        // ── Filter / sort / search ───────────────────────────────────────────────

        private void SearchBox_TextChanged(object sender, TextChangedEventArgs e)
        {
            _searchQuery = SearchBox.Text ?? "";
            ApplyFilterSort();
        }

        private void FilterChip_Click(object sender, RoutedEventArgs e)
        {
            if (sender is not ToggleButton clicked) return;
            foreach (var chip in new[] { FilterAll, FilterRecent, FilterShared, FilterUnplayed })
                chip.IsChecked = chip == clicked;
            _activeFilter = clicked.Tag as string ?? "all";
            ApplyFilterSort();
        }

        private void SortButton_Click(object sender, RoutedEventArgs e)
        {
            SortPopup.IsOpen = true;
        }

        private void SortOption_Click(object sender, RoutedEventArgs e)
        {
            if (sender is Button btn)
            {
                _sortOrder = btn.Tag as string ?? "recent";
                SortPopup.IsOpen = false;
                ApplyFilterSort();
            }
        }

        // ── Game selection ───────────────────────────────────────────────────────

        private void GameListBox_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (_updatingDetail) return;
            _updatingDetail = true;
            SelectedGame = GameListBox.SelectedItem as GameEntry;
            Dispatcher.BeginInvoke(new Action(() => _updatingDetail = false));
        }

        // ── Play ─────────────────────────────────────────────────────────────────

        private async void Play_Click(object sender, RoutedEventArgs e)
        {
            var entry = SelectedGame;
            if (entry == null) return;

            if (!File.Exists(entry.ExePath))
            {
                MessageBox.Show($"Game executable not found:\n{entry.ExePath}",
                    "Game Not Found", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            entry.LastPlayedAt = DateTime.UtcNow;
            _library.Update(entry);

            Hide();
            try
            {
                await new RunWorkflow(entry.GameName, entry.ExePath, entry: entry, library: _library).ExecuteAsync();
            }
            catch (Exception ex)
            {
                App.Log($"RunWorkflow unexpected error for '{entry.GameName}': {ex}");
                MessageBox.Show($"An unexpected error occurred: {ex.Message}",
                    "Spool Error", MessageBoxButton.OK, MessageBoxImage.Error);
            }
            finally
            {
                Show();
            }
        }

        // ── Add game ──────────────────────────────────────────────────────────────

        private void AddGame_Click(object sender, RoutedEventArgs e)
        {
            var dlg = new AddGameWindow(_config, _library);
            dlg.Owner = this;
            dlg.OnCoverArtFetched = (entry, _) => Dispatcher.Invoke(ApplyFilterSort);
            if (dlg.ShowDialog() == true)
            {
                foreach (var entry in _library.Entries)
                    if (!_games.Any(g => g.Id == entry.Id))
                        _games.Add(entry);
                ApplyFilterSort();
                _lanServer.BroadcastAnnounce();
            }
        }

        // ── Settings ──────────────────────────────────────────────────────────────

        private void Settings_Click(object sender, RoutedEventArgs e)
        {
            var setup = new SetupWindow(_config);
            setup.Owner = this;
            if (setup.ShowDialog() == true)
            {
                _lanServer.Stop();
                StartLanServerIfEnabled();
                _ = ScanLanPeersAsync();
            }
        }

        // ── Detail pane actions ───────────────────────────────────────────────────

        private void DetailOpenFolder_Click(object sender, RoutedEventArgs e)
        {
            var entry = SelectedGame;
            if (entry == null) return;
            try
            {
                string? dir = string.IsNullOrEmpty(entry.ExePath)
                    ? entry.GameFolderPath
                    : Path.GetDirectoryName(entry.ExePath);
                if (string.IsNullOrEmpty(dir) && !string.IsNullOrEmpty(entry.GameFolderPath))
                    dir = entry.GameFolderPath;
                if (dir != null && Directory.Exists(dir))
                    Process.Start("explorer.exe", $"\"{dir}\"");
                else
                    MessageBox.Show("Game folder not found.", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Could not open folder: {ex.Message}", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
            }
        }

        private async void DetailArmouryCrate_Click(object sender, RoutedEventArgs e)
        {
            var entry = SelectedGame;
            if (entry == null || entry.IsLanCard) return;

            if (!_config.IsLudusaviOk)
            {
                MessageBox.Show("Ludusavi not found - open Settings to configure it.",
                    "Ludusavi Not Found", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            try
            {
                string launcherExePath = await LauncherGenerator.GenerateLauncherExeAsync(entry, _config);
                entry.LauncherExePath = launcherExePath;
                _library.Update(entry);

                _successWindow = new SuccessWindow(this, entry.GameName, launcherExePath, SuccessMode.ArmouryCrate);
                if (_config.Data.SteamGridDbEnabled && !string.IsNullOrEmpty(_config.Data.SteamGridDbApiKey))
                {
                    _successWindow.UpdateArtwork("Fetching cover image...", "#99FFFFFF");
                    _ = Task.Run(() => FetchCoverArtForACAsync(entry.GameName, entry.SafeName));
                }
                _successWindow.ShowDialog();
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Failed to generate launcher: {ex.Message}", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
            }
        }

        private async void DetailAddToSteam_Click(object sender, RoutedEventArgs e)
        {
            var entry = SelectedGame;
            if (entry == null || entry.IsLanCard) return;

            if (!_config.IsLudusaviOk)
            {
                MessageBox.Show("Ludusavi not found - open Settings to configure it.",
                    "Ludusavi Not Found", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            string launcherExePath;
            try
            {
                launcherExePath = await LauncherGenerator.GenerateLauncherExeAsync(entry, _config);
                entry.LauncherExePath = launcherExePath;
                _library.Update(entry);
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Failed to generate launcher: {ex.Message}", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            string? steamPath = await Task.Run(() => SteamIntegration.GetSteamInstallPath());
            if (steamPath == null)
            {
                MessageBox.Show("Steam installation not found. Is Steam installed?",
                    "Steam Not Found", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            var users = await Task.Run(() => SteamIntegration.GetSteamUsers(steamPath));
            if (users.Count == 0)
            {
                MessageBox.Show("No Steam user profiles found. Launch Steam at least once to create your profile.",
                    "No Steam Users", MessageBoxButton.OK, MessageBoxImage.Error);
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
                MessageBox.Show($"Failed to read shortcuts.vdf: {ex.Message}", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            string startDir = Path.GetDirectoryName(launcherExePath) ?? "";
            SteamIntegration.UpsertShortcut(ref entries, entry.GameName, launcherExePath, startDir);

            try
            {
                await Task.Run(() => SteamIntegration.WriteShortcuts(targetUser.ShortcutsPath, entries));
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Failed to write shortcuts.vdf: {ex.Message}", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            _successWindow = new SuccessWindow(this, entry.GameName, launcherExePath, SuccessMode.Steam);
            if (users.Count > 1)
                _successWindow.UpdateArtwork($"Added to Steam user {targetUser.UserId}", "#4CAF50");
            _successWindow.ShowDialog();
        }

        private void DetailRemove_Click(object sender, RoutedEventArgs e)
        {
            var entry = SelectedGame;
            if (entry == null || entry.IsLanCard) return;

            var ans = MessageBox.Show($"Remove '{entry.GameName}' from your library?",
                "Remove Game", MessageBoxButton.YesNo, MessageBoxImage.Question);
            if (ans != MessageBoxResult.Yes) return;

            _library.Remove(entry.Id);
            var toRemove = _games.FirstOrDefault(g => g.Id == entry.Id);
            if (toRemove != null) _games.Remove(toRemove);

            _updatingDetail = true;
            SelectedGame = null;
            GameListBox.SelectedItem = null;
            Dispatcher.BeginInvoke(new Action(() => _updatingDetail = false));

            _lanServer.InvalidateManifestCache();
            _lanServer.BroadcastAnnounce();
            ApplyFilterSort();
        }

        private async void DetailDownload_Click(object sender, RoutedEventArgs e)
        {
            var entry = SelectedGame;
            if (entry == null || !entry.IsLanCard || entry.LanPeers == null || entry.LanPeers.Count == 0)
                return;

            if (_isDownloading)
            {
                MessageBox.Show("A download is already in progress. Please wait for it to finish or cancel it first.",
                    "Download in Progress", MessageBoxButton.OK, MessageBoxImage.Information);
                return;
            }

            var dlg = new DownloadLocationWindow(_config, _library, entry.GameName);
            dlg.Owner = this;
            if (dlg.ShowDialog() == true && !string.IsNullOrEmpty(dlg.DestFolder))
                await StartDownloadWithPeersAsync(entry.LanPeers, entry.GameName, dlg.DestFolder);
        }

        private async void DetailRefetchArtwork_Click(object sender, RoutedEventArgs e)
        {
            var entry = SelectedGame;
            if (entry == null) return;

            if (!_config.Data.SteamGridDbEnabled || string.IsNullOrEmpty(_config.Data.SteamGridDbApiKey))
            {
                MessageBox.Show("SteamGridDB is not enabled or API key is missing. Please configure it in Settings.",
                    "SteamGridDB Required", MessageBoxButton.OK, MessageBoxImage.Warning);
                return;
            }

            var button = (Button)sender;
            button.IsEnabled = false;

            try
            {
                var sgdb = new SteamGridDbClient(_config.Data.SteamGridDbApiKey);
                var results = await sgdb.SearchGameAsync(entry.GameName);
                if (results.Count == 0)
                {
                    MessageBox.Show($"Could not find '{entry.GameName}' on SteamGridDB.", "Game Not Found", MessageBoxButton.OK, MessageBoxImage.Information);
                    return;
                }

                int gameId = results[0].Id;
                string coversDir = Path.Combine(Config.AppDataFolder, "covers");
                Directory.CreateDirectory(coversDir);

                string destBase = Path.Combine(coversDir, entry.SafeName);

                var tPortrait = sgdb.DownloadPortraitAsync(gameId, destBase + "_p");
                var tHero = sgdb.DownloadHeroAsync(gameId, destBase + "_hero");
                await Task.WhenAll(tPortrait, tHero);

                string? imagePath = tPortrait.Result;
                imagePath ??= await sgdb.DownloadGridImageAsync(gameId, entry.SafeName, coversDir);
                string? heroPath = tHero.Result;

                if (imagePath != null || heroPath != null)
                {
                    if (imagePath != null) entry.CoverImagePath = imagePath;
                    if (heroPath != null) entry.HeroImagePath = heroPath;
                    _library.Update(entry);
                    MessageBox.Show("Artwork successfully updated!", "Success", MessageBoxButton.OK, MessageBoxImage.Information);
                }
                else
                {
                    MessageBox.Show("Failed to download artwork from SteamGridDB.", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
                }
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Failed to fetch artwork: {ex.Message}", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
            }
            finally
            {
                button.IsEnabled = true;
            }
        }

        // ── Game settings ────────────────────────────────────────────────────────

        private void RunAsAdmin_Checked(object sender, RoutedEventArgs e)
        {
            if (_updatingDetail) return;
            var entry = SelectedGame;
            if (entry == null) return;
            RegistryHelper.SetCompatFlagRunAsAdmin(entry.ExePath);
            _library.Update(entry);
        }

        private void RunAsAdmin_Unchecked(object sender, RoutedEventArgs e)
        {
            if (_updatingDetail) return;
            var entry = SelectedGame;
            if (entry == null) return;
            RegistryHelper.RemoveCompatFlagRunAsAdmin(entry.ExePath);
            _library.Update(entry);
        }

        private void LanShare_Checked(object sender, RoutedEventArgs e)
        {
            if (_updatingDetail) return;
            var entry = SelectedGame;
            if (entry == null) return;
            if (string.IsNullOrEmpty(entry.GameFolderPath) && !string.IsNullOrEmpty(entry.ExePath))
                entry.GameFolderPath = Path.GetDirectoryName(entry.ExePath);
            _library.Update(entry);
            _lanServer.InvalidateManifestCache();
            _lanServer.BroadcastAnnounce();
        }

        private void LanShare_Unchecked(object sender, RoutedEventArgs e)
        {
            if (_updatingDetail) return;
            var entry = SelectedGame;
            if (entry == null) return;
            _library.Update(entry);
            _lanServer.InvalidateManifestCache();
            _lanServer.BroadcastAnnounce();
        }

        private void LanFolderBrowse_Click(object sender, RoutedEventArgs e)
        {
            var entry = SelectedGame;
            if (entry == null) return;

            var dialog = new Microsoft.Win32.OpenFolderDialog
            {
                Title = $"Select shared folder for \"{entry.GameName}\"",
                Multiselect = false
            };
            if (dialog.ShowDialog() != true) return;

            entry.GameFolderPath = dialog.FolderName;
            _library.Update(entry);
            _lanServer.InvalidateManifestCache();
            _lanServer.BroadcastAnnounce();
        }

        // ── Save management ──────────────────────────────────────────────────────

        private async void SaveRestore_Click(object sender, RoutedEventArgs e)
        {
            var entry = SelectedGame;
            if (entry == null) return;

            if (!_config.IsLudusaviOk)
            {
                MessageBox.Show("Ludusavi not found - open Settings to configure it.",
                    "Ludusavi Not Found", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            try
            {
                var psi = new ProcessStartInfo
                {
                    FileName = _config.Data.LudusaviPath,
                    Arguments = $"restore --force \"{entry.GameName}\"",
                    UseShellExecute = false,
                    CreateNoWindow = true,
                    RedirectStandardOutput = true,
                    RedirectStandardError = true
                };
                using var proc = new Process { StartInfo = psi };
                proc.Start();
                string stdout = await proc.StandardOutput.ReadToEndAsync();
                string stderr = await proc.StandardError.ReadToEndAsync();
                await proc.WaitForExitAsync();

                if (proc.ExitCode == 0)
                    MessageBox.Show($"Saves restored for '{entry.GameName}'.",
                        "Restore Complete", MessageBoxButton.OK, MessageBoxImage.Information);
                else
                {
                    string detail = string.IsNullOrWhiteSpace(stderr) ? stdout : stderr;
                    MessageBox.Show($"Restore failed.\n{detail.Trim()}", "Restore Failed",
                        MessageBoxButton.OK, MessageBoxImage.Error);
                }
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Restore failed: {ex.Message}", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
            }
        }

        private async void SaveBackupNow_Click(object sender, RoutedEventArgs e)
        {
            var entry = SelectedGame;
            if (entry == null) return;

            if (!_config.IsLudusaviOk)
            {
                MessageBox.Show("Ludusavi not found - open Settings to configure it.",
                    "Ludusavi Not Found", MessageBoxButton.OK, MessageBoxImage.Error);
                return;
            }

            try
            {
                var psi = new ProcessStartInfo
                {
                    FileName = _config.Data.LudusaviPath,
                    Arguments = $"backup --force \"{entry.GameName}\"",
                    UseShellExecute = false,
                    CreateNoWindow = true,
                    RedirectStandardOutput = true,
                    RedirectStandardError = true
                };
                using var proc = new Process { StartInfo = psi };
                proc.Start();
                string stdout = await proc.StandardOutput.ReadToEndAsync();
                string stderr = await proc.StandardError.ReadToEndAsync();
                await proc.WaitForExitAsync();

                if (proc.ExitCode == 0)
                {
                    entry.SaveLastBackedUpAt = DateTime.UtcNow;
                    entry.SaveBackupCount = Math.Max(1, entry.SaveBackupCount + 1);
                    _library.Update(entry);
                    UpdateOverviewStats();
                    MessageBox.Show($"Saves backed up for '{entry.GameName}'.",
                        "Backup Complete", MessageBoxButton.OK, MessageBoxImage.Information);
                }
                else
                {
                    string detail = string.IsNullOrWhiteSpace(stderr) ? stdout : stderr;
                    MessageBox.Show($"Backup failed.\n{detail.Trim()}", "Backup Failed",
                        MessageBoxButton.OK, MessageBoxImage.Error);
                }
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Backup failed: {ex.Message}", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
            }
        }

        // ── Cover art helpers ────────────────────────────────────────────────────

        private async Task FetchCoverArtForACAsync(string gameName, string safeName)
        {
            string coversDir = Path.Combine(Config.AppDataFolder, "covers");
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
                App.Log($"[MainWindow] Failed to fetch cover art for Armoury Crate '{gameName}': {ex.Message}");
                Dispatcher.Invoke(() => _successWindow?.UpdateArtwork($"⚠ Artwork error: {ex.Message}", "#FFC107"));
            }
        }

        private async Task FetchCoverForLanEntryAsync(GameEntry entry, LanPeer? peer)
        {
            string coversDir = Path.Combine(Config.AppDataFolder, "covers");
            Directory.CreateDirectory(coversDir);

            if (peer != null)
            {
                try
                {
                    using var http = new System.Net.Http.HttpClient { Timeout = TimeSpan.FromSeconds(15) };
                    string url = $"http://{peer.IPAddress}:{peer.Port}/games/{Uri.EscapeDataString(entry.GameName)}/cover";
                    using var response = await http.GetAsync(url);
                    if (response.IsSuccessStatusCode)
                    {
                        string ext = response.Content.Headers.ContentType?.MediaType switch
                        {
                            "image/png"  => ".png",
                            "image/webp" => ".webp",
                            "image/gif"  => ".gif",
                            _            => ".jpg"
                        };
                        string imagePath = Path.Combine(coversDir, entry.SafeName + "_p" + ext);
                        byte[] bytes = await response.Content.ReadAsByteArrayAsync();
                        await File.WriteAllBytesAsync(imagePath, bytes);
                        Dispatcher.Invoke(() => { entry.CoverImagePath = imagePath; _library.Update(entry); });
                        App.Log($"Downloaded cover from LAN peer for '{entry.GameName}'");
                        return;
                    }
                }
                catch (Exception ex)
                {
                    App.Log($"LAN cover fetch failed for '{entry.GameName}': {ex.Message}");
                }
            }

            if (!_config.Data.SteamGridDbEnabled || string.IsNullOrEmpty(_config.Data.SteamGridDbApiKey)) return;
            try
            {
                var sgdb = new SteamGridDbClient(_config.Data.SteamGridDbApiKey);
                var results = await sgdb.SearchGameAsync(entry.GameName);
                if (results.Count == 0) return;

                string destBase = Path.Combine(coversDir, entry.SafeName);
                var tPortrait = sgdb.DownloadPortraitAsync(results[0].Id, destBase + "_p");
                var tHero = sgdb.DownloadHeroAsync(results[0].Id, destBase + "_hero");
                await Task.WhenAll(tPortrait, tHero);

                string? imagePath = tPortrait.Result;
                imagePath ??= await sgdb.DownloadGridImageAsync(results[0].Id, entry.SafeName, coversDir);
                string? heroPath = tHero.Result;

                if (imagePath != null || heroPath != null)
                {
                    Dispatcher.Invoke(() =>
                    {
                        if (imagePath != null) entry.CoverImagePath = imagePath;
                        if (heroPath != null) entry.HeroImagePath = heroPath;
                        _library.Update(entry);
                    });
                }
            }
            catch (Exception ex)
            {
                App.Log($"SteamGridDB cover fetch failed for '{entry.GameName}': {ex.Message}");
            }
        }

        // ── LAN server / scanning ────────────────────────────────────────────────

        private void StartLanServerIfEnabled()
        {
            if (!_config.Data.LanShareEnabled) return;
            try
            {
                _lanServer.Start(() => _library.Entries, _config.Data.LanSharePort);
            }
            catch (Exception ex)
            {
                App.Log($"LAN server failed to start: {ex.Message}");
            }
        }

        private async Task RunLanScanLoopAsync()
        {
            while (_scanCts != null && !_scanCts.IsCancellationRequested)
            {
                await ScanLanPeersAsync();
                try
                {
                    await Task.Delay(TimeSpan.FromSeconds(30), _scanCts.Token);
                }
                catch (OperationCanceledException)
                {
                    break;
                }
            }
        }

        private async Task ScanLanPeersAsync()
        {
            if (Interlocked.CompareExchange(ref _scanInProgress, 1, 0) != 0) return;
            Dispatcher.Invoke(() => IsLanScanning = true);
            try
            {
                int discoveryPort = _config.Data.LanSharePort - 1;
                var peers = await _lanClient.DiscoverPeersAsync(discoveryPort, _scanCts?.Token ?? default);
                Dispatcher.Invoke(() => LanPeersCount = peers.Count);
                MergeLanGames(peers);
            }
            catch (Exception ex)
            {
                App.Log($"Error during background LAN scan: {ex.Message}");
            }
            finally
            {
                Interlocked.Exchange(ref _scanInProgress, 0);
                Dispatcher.Invoke(() => IsLanScanning = false);
            }
        }

        private void LanServer_PeerActivityDetected(object? sender, EventArgs e)
        {
            _ = ScanLanPeersAsync();
        }

        private void MergeLanGames(List<LanPeer> peers)
        {
            var lanGames = new Dictionary<string, List<LanPeer>>(StringComparer.OrdinalIgnoreCase);
            foreach (var peer in peers)
                foreach (var gameName in peer.Games)
                {
                    if (!lanGames.TryGetValue(gameName, out var peerList))
                    {
                        peerList = new List<LanPeer>();
                        lanGames[gameName] = peerList;
                    }
                    peerList.Add(peer);
                }

            var localInstalled = _library.Entries
                .Where(e => !string.IsNullOrEmpty(e.GameFolderPath) && Directory.Exists(e.GameFolderPath))
                .Select(e => e.GameName)
                .ToHashSet(StringComparer.OrdinalIgnoreCase);

            Dispatcher.BeginInvoke(new Action(() =>
            {
                var toRemove = _games
                    .Where(g => g.IsLanCard && (!lanGames.ContainsKey(g.GameName) || localInstalled.Contains(g.GameName)))
                    .ToList();
                foreach (var g in toRemove) _games.Remove(g);

                foreach (var kvp in lanGames)
                {
                    string gameName = kvp.Key;
                    if (localInstalled.Contains(gameName)) continue;

                    var gamePeers = kvp.Value;
                    var firstPeer = gamePeers.First();
                    string coverUrl = $"http://{firstPeer.IPAddress}:{firstPeer.Port}/games/{Uri.EscapeDataString(gameName)}/cover";

                    var existing = _games.FirstOrDefault(g => g.IsLanCard && string.Equals(g.GameName, gameName, StringComparison.OrdinalIgnoreCase));
                    if (existing != null)
                    {
                        existing.LanPeers = gamePeers;
                        existing.CoverImagePath = coverUrl;
                    }
                    else
                    {
                        _games.Add(new GameEntry
                        {
                            GameName = gameName,
                            SafeName = LauncherGenerator.MakeSafeFilename(gameName),
                            CoverImagePath = coverUrl,
                            IsLanCard = true,
                            LanPeers = gamePeers
                        });
                    }
                }

                ApplyFilterSort();
            }));
        }

        // ── Upload progress ──────────────────────────────────────────────────────

        private System.Windows.Threading.DispatcherTimer? _uploadTimer;
        private long _lastBytesSent = 0;
        private DateTime _lastUploadTick = DateTime.MinValue;

        private void LanServer_UploadsChanged(object? sender, EventArgs e)
        {
            if (_uploadTimer == null)
                Dispatcher.BeginInvoke(new Action(UpdateUploadProgress));
        }

        private void UpdateUploadProgress()
        {
            var uploads = _lanServer.GetActiveUploads();
            if (uploads.Count == 0)
            {
                _uploadTimer?.Stop();
                _uploadTimer = null;
                UploadSeparator.Visibility = Visibility.Collapsed;
                UploadBarGrid.Visibility = Visibility.Collapsed;
                _lastBytesSent = 0;
                _lastUploadTick = DateTime.MinValue;
                return;
            }

            if (_uploadTimer == null)
            {
                _uploadTimer = new System.Windows.Threading.DispatcherTimer
                {
                    Interval = TimeSpan.FromMilliseconds(500)
                };
                _uploadTimer.Tick += (s, e) => UpdateUploadProgress();
                _uploadTimer.Start();
                _lastUploadTick = DateTime.UtcNow;
                _lastBytesSent = 0;
            }

            long totalBytes = 0, bytesSent = 0;
            string gameName = "";
            foreach (var u in uploads)
            {
                totalBytes += u.TotalBytes;
                bytesSent  += u.BytesSent;
                if (string.IsNullOrEmpty(gameName)) gameName = u.GameName;
            }

            var gameNames = uploads.Select(u => u.GameName).Distinct().ToList();
            UploadTitleText.Text = gameNames.Count > 1
                ? $"Uploading {gameNames.Count} games to LAN peer..."
                : $"Uploading {gameName} to LAN peer...";

            UploadProgressBar.Value = totalBytes > 0 ? bytesSent * 100.0 / totalBytes : 0;

            double speed = 0;
            var now = DateTime.UtcNow;
            if (_lastUploadTick != DateTime.MinValue && bytesSent >= _lastBytesSent)
            {
                double elapsed = (now - _lastUploadTick).TotalSeconds;
                if (elapsed > 0) speed = Math.Max(0, (bytesSent - _lastBytesSent) / elapsed);
            }
            _lastBytesSent = bytesSent;
            _lastUploadTick = now;

            UploadStatusText.Text = $"{FormatSpeed(speed)} - {FormatBytes(bytesSent)} of {FormatBytes(totalBytes)}";
            UploadSeparator.Visibility = Visibility.Visible;
            UploadBarGrid.Visibility = Visibility.Visible;
        }

        private void CancelUpload_Click(object sender, RoutedEventArgs e)
        {
            _lanServer.CancelAllUploads();
        }

        // ── Download (LAN) ───────────────────────────────────────────────────────

        private CancellationTokenSource? _downloadCts;
        private bool _isDownloading = false;

        private async Task StartDownloadWithPeersAsync(List<LanPeer> peers, string gameName, string destFolder)
        {
            _isDownloading = true;
            DownloadSeparator.Visibility = Visibility.Visible;
            DownloadBarGrid.Visibility = Visibility.Visible;
            CancelDownloadButton.Visibility = Visibility.Visible;

            DownloadTitleText.Text = "Preparing...";
            DownloadCountText.Text = "";
            DownloadSpeedText.Text = "";
            DownloadBytesText.Text = "";
            DownloadProgressBar.Value = 0;

            _downloadCts?.Dispose();
            _downloadCts = new CancellationTokenSource();
            var ct = _downloadCts.Token;

            var progress = new Progress<LanDownloadProgress>(p =>
            {
                if (!string.IsNullOrEmpty(p.Status) && p.Status != "Downloading")
                {
                    DownloadTitleText.Text = p.Status;
                    if (p.Status.StartsWith("Verifying"))
                    {
                        DownloadBytesText.Text = Path.GetFileName(p.CurrentFile);
                        DownloadProgressBar.Value = p.TotalBytes > 0 ? (double)p.BytesTransferred / p.TotalBytes * 100 : 0;
                    }
                    else
                    {
                        DownloadBytesText.Text = "";
                        DownloadProgressBar.Value = 0;
                    }
                    DownloadCountText.Text = "";
                    DownloadSpeedText.Text = "";
                }
                else
                {
                    DownloadTitleText.Text = $"Downloading {Path.GetFileName(p.CurrentFile)}";
                    DownloadCountText.Text = $"({p.FilesCompleted} / {p.TotalFiles} files)";
                    DownloadProgressBar.Value = p.TotalBytes > 0 ? (double)p.BytesTransferred / p.TotalBytes * 100 : 0;
                    DownloadBytesText.Text = $"{FormatBytes(p.BytesTransferred)} of {FormatBytes(p.TotalBytes)}";
                    DownloadSpeedText.Text = FormatSpeed(p.SpeedBytesPerSec);
                }
            });

            Exception? lastEx = null;
            bool success = false;
            LanPeer? successPeer = null;
            try
            {
                foreach (var peer in peers)
                {
                    if (ct.IsCancellationRequested) break;
                    try
                    {
                        App.Log($"Attempting LAN download from peer: {peer.DeviceName} ({peer.IPAddress}:{peer.Port})");
                        await _lanClient.DownloadGameAsync(peer, gameName, destFolder, progress, ct);
                        success = true;
                        successPeer = peer;
                        break;
                    }
                    catch (OperationCanceledException) { throw; }
                    catch (Exception ex)
                    {
                        App.Log($"Download from peer {peer.DeviceName} failed: {ex.Message}");
                        lastEx = ex;
                    }
                }

                if (success)
                {
                    DownloadTitleText.Text = "Download complete";
                    DownloadCountText.Text = "";
                    DownloadProgressBar.Value = 100;
                    DownloadSpeedText.Text = "";
                    DownloadBytesText.Text = "";
                    OfferAddToLibrary(gameName, destFolder, successPeer);
                }
                else if (ct.IsCancellationRequested)
                {
                    DownloadTitleText.Text = "Download cancelled";
                    DownloadCountText.Text = "";
                    DownloadSpeedText.Text = "";
                    DownloadBytesText.Text = "";
                }
                else
                {
                    MessageBox.Show($"Download failed: {lastEx?.Message ?? "No peers available"}", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
                    DownloadTitleText.Text = "Download failed";
                    DownloadCountText.Text = "";
                    DownloadSpeedText.Text = "";
                    DownloadBytesText.Text = "";
                }
            }
            catch (OperationCanceledException)
            {
                bool hostCancelled = !ct.IsCancellationRequested;
                DownloadTitleText.Text = hostCancelled ? "Cancelled by host" : "Download cancelled";
                DownloadCountText.Text = "";
                DownloadSpeedText.Text = "";
                DownloadBytesText.Text = "";
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Download failed: {ex.Message}", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
                DownloadTitleText.Text = "Download failed";
                DownloadCountText.Text = "";
                DownloadSpeedText.Text = "";
                DownloadBytesText.Text = "";
            }
            finally
            {
                _isDownloading = false;
                CancelDownloadButton.Visibility = Visibility.Collapsed;
                _uiCleanupCts?.Cancel();
                _uiCleanupCts?.Dispose();
                _uiCleanupCts = new CancellationTokenSource();
                var cleanupToken = _uiCleanupCts.Token;
                _ = Task.Delay(TimeSpan.FromSeconds(5), cleanupToken).ContinueWith(t =>
                {
                    if (cleanupToken.IsCancellationRequested) return;
                    Dispatcher.Invoke(() =>
                    {
                        if (!_isDownloading)
                        {
                            DownloadSeparator.Visibility = Visibility.Collapsed;
                            DownloadBarGrid.Visibility = Visibility.Collapsed;
                        }
                    });
                }, TaskScheduler.Default);
            }
        }

        private async void OfferAddToLibrary(string gameName, string destFolder, LanPeer? sourcePeer = null)
        {
            try { await OfferAddToLibraryAsync(gameName, destFolder, sourcePeer); }
            catch (Exception ex) { App.Log($"OfferAddToLibrary failed for '{gameName}': {ex}"); }
        }

        private async Task OfferAddToLibraryAsync(string gameName, string destFolder, LanPeer? sourcePeer)
        {
            bool runAsAdmin = false;
            string? relativeExePath = null;

            if (sourcePeer != null)
            {
                try
                {
                    var meta = await _lanClient.GetMetadataAsync(sourcePeer, gameName);
                    if (meta != null)
                    {
                        runAsAdmin = meta.RunAsAdmin;
                        relativeExePath = meta.RelativeExePath;
                    }
                }
                catch (Exception ex)
                {
                    App.Log($"Failed to fetch LAN metadata for '{gameName}': {ex.Message}");
                }
            }

            string? exePath = null;
            if (!string.IsNullOrEmpty(relativeExePath))
            {
                string candidate = Path.Combine(destFolder, relativeExePath.Replace('/', Path.DirectorySeparatorChar));
                if (File.Exists(candidate)) exePath = candidate;
            }
            if (exePath == null)
            {
                exePath = Directory.GetFiles(destFolder, "*.exe", SearchOption.AllDirectories)
                    .OrderBy(f => Path.GetDirectoryName(f)?.Length)
                    .FirstOrDefault();
            }

            var existing = _library.FindByName(gameName);
            if (existing != null)
            {
                existing.GameFolderPath = destFolder;
                if (exePath != null)
                {
                    existing.ExePath = exePath;
                    existing.RunAsAdmin = runAsAdmin;
                    if (runAsAdmin) RegistryHelper.SetCompatFlagRunAsAdmin(exePath);
                }
                _library.Update(existing);
            }
            else
            {
                var newEntry = new GameEntry
                {
                    GameName       = gameName,
                    SafeName       = LauncherGenerator.MakeSafeFilename(gameName),
                    ExePath        = exePath ?? "",
                    GameFolderPath = destFolder,
                    RunAsAdmin     = runAsAdmin,
                    AddedAt        = DateTime.UtcNow
                };
                _library.Add(newEntry);
                if (runAsAdmin && exePath != null)
                    RegistryHelper.SetCompatFlagRunAsAdmin(exePath);
            }

            LoadGames();
            ShowToast("Transfer Complete", $"\"{gameName}\" was automatically added to your library.");

            var entry = _library.FindByName(gameName);
            if (entry != null && string.IsNullOrEmpty(entry.CoverImagePath))
                _ = Task.Run(() => FetchCoverForLanEntryAsync(entry, sourcePeer));
        }

        // ── TorBox download ───────────────────────────────────────────────────────

        private void Browse_Click(object sender, RoutedEventArgs e)
        {
            if (_config.Data.DownloadSources.Count == 0)
            {
                MessageBox.Show("No download sources configured.\n\nAdd Hydra-compatible JSON source URLs in Settings.",
                    "No Sources", MessageBoxButton.OK, MessageBoxImage.Information);
                return;
            }
            if (_isDownloading)
            {
                MessageBox.Show("A download is already in progress. Please wait for it to finish or cancel it first.",
                    "Download in Progress", MessageBoxButton.OK, MessageBoxImage.Information);
                return;
            }
            var browse = new BrowseWindow(_config) { Owner = this };
            if (browse.ShowDialog() == true && browse.SelectedDownload != null)
            {
                if (!_config.IsTorBoxOk)
                {
                    MessageBox.Show("TorBox is not configured.\n\nEnable TorBox and enter your API key in Settings.",
                        "TorBox Not Configured", MessageBoxButton.OK, MessageBoxImage.Warning);
                    return;
                }
                _ = StartTorBoxDownloadAsync(browse.SelectedDownload);
            }
        }

        private async Task StartTorBoxDownloadAsync(HydraDownloadEntry download)
        {
            _isDownloading = true;
            DownloadSeparator.Visibility = Visibility.Visible;
            DownloadBarGrid.Visibility = Visibility.Visible;
            CancelDownloadButton.Visibility = Visibility.Visible;

            DownloadTitleText.Text = "Preparing...";
            DownloadCountText.Text = "";
            DownloadSpeedText.Text = "";
            DownloadBytesText.Text = "";
            DownloadProgressBar.Value = 0;

            _downloadCts?.Dispose();
            _downloadCts = new CancellationTokenSource();
            var ct = _downloadCts.Token;

            string? destDir = null;
            try
            {
                var torbox = new TorBoxClient(_config.Data.TorBoxApiKey);
                string magnet = download.Uris.Find(u => u.StartsWith("magnet:", StringComparison.OrdinalIgnoreCase))
                    ?? download.Uris[0];

                DownloadTitleText.Text = "Adding to TorBox...";
                int torrentId = await torbox.AddMagnetAsync(magnet);

                DownloadTitleText.Text = "Waiting for TorBox...";
                TorBoxTorrent? torrent = null;
                while (!ct.IsCancellationRequested)
                {
                    torrent = await torbox.GetTorrentInfoAsync(torrentId);
                    if (torrent == null) throw new Exception("Torrent not found in TorBox");
                    if (torrent.DownloadState is "cached" or "completed" or "uploading") break;
                    DownloadTitleText.Text = $"TorBox: {torrent.DownloadState}...";
                    DownloadProgressBar.Value = torrent.Progress * 100;
                    await Task.Delay(5000, ct);
                }
                ct.ThrowIfCancellationRequested();

                var files = torrent!.Files ?? new List<TorBoxFile>();
                if (files.Count == 0) throw new Exception("No files found in torrent");

                string safeTitle = LauncherGenerator.MakeSafeFilename(download.Title);
                destDir = System.IO.Path.Combine(_config.EffectiveDownloadDir, safeTitle);

                string? commonRoot = null;
                bool shareCommonRoot = files.Count > 0;
                foreach (var file in files)
                {
                    int slashIdx = file.Name.IndexOf('/');
                    if (slashIdx <= 0) { shareCommonRoot = false; break; }
                    string root = file.Name[..slashIdx];
                    if (commonRoot == null) commonRoot = root;
                    else if (commonRoot != root) { shareCommonRoot = false; break; }
                }

                long totalBytesAllFiles = files.Sum(f => f.Size);
                long totalBytesDownloaded = 0, lastBytes = 0;
                DateTime lastTick = DateTime.UtcNow;
                int currentFileIndex = 0;

                foreach (var file in files)
                {
                    currentFileIndex++;
                    ct.ThrowIfCancellationRequested();

                    string relativePath = file.Name;
                    if (shareCommonRoot && commonRoot != null && relativePath.StartsWith(commonRoot + "/"))
                        relativePath = relativePath[(commonRoot.Length + 1)..];
                    string normalizedPath = relativePath.Replace('/', System.IO.Path.DirectorySeparatorChar);
                    string destPath = System.IO.Path.Combine(destDir, normalizedPath);

                    DownloadTitleText.Text = $"[{currentFileIndex}/{files.Count}] Getting link for {file.ShortName}...";
                    DownloadCountText.Text = $"File {currentFileIndex} of {files.Count}";
                    string link = await torbox.RequestDownloadLinkAsync(torrentId, file.Id);

                    var fileProgress = new Progress<(long bytes, long total)>(p =>
                    {
                        long cur = totalBytesDownloaded + p.bytes;
                        DownloadProgressBar.Value = totalBytesAllFiles > 0 ? (double)cur / totalBytesAllFiles * 100 : 0;
                        DownloadTitleText.Text = $"Downloading {file.ShortName}";
                        DownloadBytesText.Text = totalBytesAllFiles > 0
                            ? $"{FormatBytes(cur)} of {FormatBytes(totalBytesAllFiles)}"
                            : FormatBytes(cur);
                        var now = DateTime.UtcNow;
                        double elapsed = (now - lastTick).TotalSeconds;
                        if (elapsed >= 1.0)
                        {
                            DownloadSpeedText.Text = FormatSpeed((cur - lastBytes) / elapsed);
                            lastBytes = cur;
                            lastTick = now;
                        }
                    });

                    await torbox.DownloadFileAsync(link, destPath, fileProgress, ct);
                    totalBytesDownloaded += file.Size;
                }

                DownloadTitleText.Text = "Download complete";
                DownloadCountText.Text = "";
                DownloadSpeedText.Text = "";
                DownloadBytesText.Text = "";
                DownloadProgressBar.Value = 100;

                if (destDir != null && IsFitGirlRepack(download, destDir))
                    HandleFitGirlRepack(download.Title, destDir);
                else
                    OfferAddToLibrary(download.Title, destDir!);
            }
            catch (OperationCanceledException)
            {
                DownloadTitleText.Text = "Download cancelled";
                DownloadCountText.Text = "";
                DownloadSpeedText.Text = "";
                DownloadBytesText.Text = "";
            }
            catch (Exception ex)
            {
                App.Log($"TorBox download failed: {ex.Message}");
                MessageBox.Show($"Download failed: {ex.Message}", "Error", MessageBoxButton.OK, MessageBoxImage.Error);
                DownloadTitleText.Text = "Download failed";
                DownloadCountText.Text = "";
                DownloadSpeedText.Text = "";
                DownloadBytesText.Text = "";
            }
            finally
            {
                _isDownloading = false;
                _ = Task.Delay(TimeSpan.FromSeconds(4)).ContinueWith(_ =>
                {
                    Dispatcher.Invoke(() =>
                    {
                        if (!_isDownloading)
                        {
                            DownloadSeparator.Visibility = Visibility.Collapsed;
                            DownloadBarGrid.Visibility = Visibility.Collapsed;
                        }
                    });
                }, TaskScheduler.Default);
            }
        }

        private static bool IsFitGirlRepack(HydraDownloadEntry download, string destDir)
        {
            if (download.SourceName.Contains("fitgirl", StringComparison.OrdinalIgnoreCase)) return true;
            return File.Exists(Path.Combine(destDir, "setup.exe")) &&
                   Directory.GetFiles(destDir, "fg-*.bin").Length > 0;
        }

        private void HandleFitGirlRepack(string title, string destDir)
        {
            string setupExe = Path.Combine(destDir, "setup.exe");
            if (!File.Exists(setupExe))
            {
                MessageBox.Show(
                    $"FitGirl repack detected for \"{title}\" but setup.exe was not found in the download folder.\n\n{destDir}",
                    "Setup Not Found", MessageBoxButton.OK, MessageBoxImage.Warning);
                return;
            }
            var result = MessageBox.Show(
                $"FitGirl repack detected for \"{title}\".\n\nRun setup.exe as administrator to install the game?",
                "Install Game", MessageBoxButton.YesNo, MessageBoxImage.Question);
            if (result != MessageBoxResult.Yes) return;
            try
            {
                Process.Start(new ProcessStartInfo
                {
                    FileName = setupExe,
                    UseShellExecute = true,
                    Verb = "runas"
                });
            }
            catch (Exception ex)
            {
                MessageBox.Show($"Failed to launch setup.exe: {ex.Message}", "Error",
                    MessageBoxButton.OK, MessageBoxImage.Error);
            }
        }

        private void CancelDownload_Click(object sender, RoutedEventArgs e)
        {
            _downloadCts?.Cancel();
        }

        // ── Utilities ────────────────────────────────────────────────────────────

        private static string FormatBytes(long bytes)
        {
            string[] s = { "B", "KB", "MB", "GB", "TB" };
            double val = bytes;
            int i = 0;
            while (val >= 1024 && i < s.Length - 1) { val /= 1024; i++; }
            return $"{val:0.0} {s[i]}";
        }

        private static string FormatSpeed(double bytesPerSec)
        {
            string[] s = { "B/s", "KB/s", "MB/s", "GB/s" };
            double val = bytesPerSec;
            int i = 0;
            while (val >= 1024 && i < s.Length - 1) { val /= 1024; i++; }
            return $"{val:0.0} {s[i]}";
        }

        private static void ShowToast(string title, string body)
        {
            try
            {
                new ToastContentBuilder().AddText(title).AddText(body).Show();
            }
            catch (Exception ex)
            {
                App.Log($"Toast notification failed: {ex.Message}");
            }
        }

        // ── Window lifecycle ─────────────────────────────────────────────────────

        private void MainWindow_Loaded(object sender, RoutedEventArgs e)
        {
            StartLanServerIfEnabled();
            _scanCts = new CancellationTokenSource();
            _ = RunLanScanLoopAsync();

            if (System.Version.TryParse(Version, out var parsedVersion))
                AutoUpdaterDotNET.AutoUpdater.InstalledVersion = parsedVersion;

            AutoUpdaterDotNET.AutoUpdater.ShowSkipButton = false;
            AutoUpdaterDotNET.AutoUpdater.ShowRemindLaterButton = false;
            AutoUpdaterDotNET.AutoUpdater.RunUpdateAsAdmin = false;
            AutoUpdaterDotNET.AutoUpdater.SetOwner(this);

            if (!_updateCheckSubscribed)
            {
                _updateCheckSubscribed = true;
                AutoUpdaterDotNET.AutoUpdater.CheckForUpdateEvent += (args) =>
                {
                    if (args.Error != null || !args.IsUpdateAvailable) return;
                    var result = MessageBox.Show(
                        $"Version {args.CurrentVersion} is available (you have {args.InstalledVersion}).\n\nInstall now? The app will restart automatically.",
                        "Update Available", MessageBoxButton.YesNo, MessageBoxImage.Information);
                    if (result != MessageBoxResult.Yes) return;
                    try
                    {
                        args.InstallerArgs = "/VERYSILENT /SUPPRESSMSGBOXES";
                        if (AutoUpdaterDotNET.AutoUpdater.DownloadUpdate(args))
                            Application.Current.Shutdown();
                    }
                    catch (Exception ex)
                    {
                        MessageBox.Show($"Update failed: {ex.Message}", "Update Error",
                            MessageBoxButton.OK, MessageBoxImage.Error);
                    }
                };
            }

            AutoUpdaterDotNET.AutoUpdater.Start(
                "https://raw.githubusercontent.com/aidankinzett/Spool/master/update.xml");
        }

        private void MainWindow_Closing(object? sender, System.ComponentModel.CancelEventArgs e)
        {
            _scanCts?.Cancel();
            _scanCts?.Dispose();
            _uiCleanupCts?.Cancel();
            _uiCleanupCts?.Dispose();
            _downloadCts?.Dispose();
            _lanServer.Stop();
        }
    }
}
