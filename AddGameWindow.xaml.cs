using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Text.RegularExpressions;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using Microsoft.Win32;

namespace LudusaviWrap
{
    enum AddGameState { Empty, Detecting, MultiMatch, SingleMatch, NoMatch }

    public class LudusaviCandidate : INotifyPropertyChanged
    {
        public string Name { get; set; } = "";
        public int Confidence { get; set; }
        public bool IsBest { get; set; }

        private bool _isSelected;
        public bool IsSelected
        {
            get => _isSelected;
            set { _isSelected = value; Notify(); Notify(nameof(RadioFillVisibility)); Notify(nameof(RadioBorderColor)); Notify(nameof(NameWeight)); Notify(nameof(RowBackground)); Notify(nameof(RowBorderBrush)); }
        }

        private bool _isExpanded;
        public bool IsExpanded
        {
            get => _isExpanded;
            set { _isExpanded = value; Notify(); Notify(nameof(PathsVisibility)); Notify(nameof(ChevronText)); }
        }

        private List<string>? _savePaths;
        public List<string>? SavePaths
        {
            get => _savePaths;
            set { _savePaths = value; Notify(); Notify(nameof(LoadingPathsVisibility)); }
        }

        private bool _loadingPaths;
        public bool LoadingPaths
        {
            get => _loadingPaths;
            set { _loadingPaths = value; Notify(nameof(LoadingPathsVisibility)); }
        }

        public Visibility PathsVisibility => IsExpanded ? Visibility.Visible : Visibility.Collapsed;
        public Visibility LoadingPathsVisibility => (IsExpanded && LoadingPaths) ? Visibility.Visible : Visibility.Collapsed;
        public Visibility BestMatchVisibility => IsBest ? Visibility.Visible : Visibility.Collapsed;
        public Visibility RadioFillVisibility => IsSelected ? Visibility.Visible : Visibility.Collapsed;
        public string ChevronText => IsExpanded ? "▲" : "▼";
        public string NameWeight => IsSelected ? "Medium" : "Normal";
        public string ConfidenceLabel => $"{Confidence}% match";
        public string SaveCountLabel => SavePaths != null ? $"{SavePaths.Count} save locations" : "Checking save locations…";
        public double ConfidenceBarWidth => Confidence * 64.0 / 100.0;
        public Color ConfidenceBarColor => Confidence > 70 ? Color.FromRgb(0x4C, 0xC2, 0xFF) : Color.FromArgb(140, 255, 255, 255);
        public Color AccentColor => Color.FromRgb(0x4C, 0xC2, 0xFF);
        public Color RadioBorderColor => IsSelected ? Color.FromRgb(0x4C, 0xC2, 0xFF) : Color.FromArgb(80, 255, 255, 255);

        // Row background / border updated by selection state (set from code-behind after binding resolves)
        public SolidColorBrush RowBackground => IsSelected
            ? new SolidColorBrush(Color.FromArgb(0x14, 0x4C, 0xC2, 0xFF))
            : new SolidColorBrush(Color.FromArgb(0x06, 0xFF, 0xFF, 0xFF));
        public SolidColorBrush RowBorderBrush => IsSelected
            ? new SolidColorBrush(Color.FromArgb(0x55, 0x4C, 0xC2, 0xFF))
            : new SolidColorBrush(Color.FromArgb(0x10, 0xFF, 0xFF, 0xFF));

        public event PropertyChangedEventHandler? PropertyChanged;
        private void Notify([System.Runtime.CompilerServices.CallerMemberName] string? n = null)
            => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(n));
    }

    public partial class AddGameWindow : Wpf.Ui.Controls.FluentWindow
    {
        private readonly Config _config;
        private readonly GameLibrary _library;
        private SuccessWindow? _successWindow;

        public Action<GameEntry, string>? OnCoverArtFetched { get; set; }

        // ── State machine ──────────────────────────────────────────────────────
        private AddGameState _state = AddGameState.Empty;
        private string? _exePath;
        private List<LudusaviCandidate> _candidates = new();
        private LudusaviCandidate? _singleMatch;
        private bool _singlePathsExpanded;
        private bool _addWithoutSaveManagement;

        // Separate list of all detected candidates (used when going from single → multi)
        private List<LudusaviCandidate> _allCandidates = new();

        public AddGameWindow(Config config, GameLibrary library)
        {
            InitializeComponent();
            _config = config;
            _library = library;
            ApplyState(AddGameState.Empty);
            ApplyTouchLayout();
        }

        private void ApplyTouchLayout()
        {
            bool touch = (Application.Current as App)?.IsTouchOptimized ?? false;
            Width = touch ? 760 : 680;
            double btnH = touch ? 48 : 36;
            GenerateArmouryCrateButton.Height = btnH;
            AddToSteamButton.Height = btnH;
            AddToLibraryButton.Height = btnH;
        }

        // ── Drag-and-drop ──────────────────────────────────────────────────────
        private void Window_DragOver(object sender, DragEventArgs e)
        {
            if (_state != AddGameState.Empty) { e.Effects = DragDropEffects.None; e.Handled = true; return; }
            if (e.Data.GetDataPresent(DataFormats.FileDrop))
            {
                var files = (string[])e.Data.GetData(DataFormats.FileDrop);
                if (files != null && files.Any(f => f.EndsWith(".exe", StringComparison.OrdinalIgnoreCase)))
                {
                    e.Effects = DragDropEffects.Copy;
                    SetDraggingStyle(true);
                    e.Handled = true;
                    return;
                }
            }
            e.Effects = DragDropEffects.None;
            e.Handled = true;
        }

        private void Window_DragLeave(object sender, DragEventArgs e)
        {
            SetDraggingStyle(false);
        }

        private void Window_Drop(object sender, DragEventArgs e)
        {
            SetDraggingStyle(false);
            if (!e.Data.GetDataPresent(DataFormats.FileDrop)) return;
            var files = (string[])e.Data.GetData(DataFormats.FileDrop);
            string? exe = files?.FirstOrDefault(f => f.EndsWith(".exe", StringComparison.OrdinalIgnoreCase));
            if (exe != null) AcceptExe(exe);
        }

        private void SetDraggingStyle(bool dragging)
        {
            if (dragging)
            {
                DropZoneBorder.BorderBrush = (Brush)Application.Current.Resources["SystemAccentColorPrimaryBrush"];
                DropZoneTitle.Text = "Drop to identify";
                DropZoneSubtitle.Inlines.Clear();
                DropZoneSubtitle.Inlines.Add(new System.Windows.Documents.Run("Spool will look it up in ludusavi's database")
                {
                    Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"]
                });
            }
            else
            {
                DropZoneBorder.BorderBrush = new SolidColorBrush(Color.FromArgb(48, 255, 255, 255));
                DropZoneTitle.Text = "Drop a game .exe here";
                DropZoneSubtitle.Inlines.Clear();
                DropZoneSubtitle.Inlines.Add(new System.Windows.Documents.Run("or "));
                DropZoneSubtitle.Inlines.Add(new System.Windows.Documents.Run("browse for one")
                {
                    Foreground = (Brush)Application.Current.Resources["SystemAccentColorPrimaryBrush"],
                    TextDecorations = TextDecorations.Underline
                });
            }
        }

        // ── Browse / change ────────────────────────────────────────────────────
        private void DropZone_Click(object sender, System.Windows.Input.MouseButtonEventArgs e)
        {
            if (_state != AddGameState.Empty) return;
            BrowseForExe();
        }

        private void ChangeExe_Click(object sender, RoutedEventArgs e) => ResetToEmpty();

        private void BrowseForExe()
        {
            var dialog = new OpenFileDialog
            {
                Title = "Select game executable",
                Filter = "Executable (*.exe)|*.exe|All files (*.*)|*.*",
                RestoreDirectory = true
            };
            if (dialog.ShowDialog() == true)
                AcceptExe(dialog.FileName);
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

        // ── Core detection flow ────────────────────────────────────────────────
        private void AcceptExe(string path)
        {
            _exePath = path;
            _addWithoutSaveManagement = false;

            // Populate EXE card info
            ExeFilenameLabel.Text = Path.GetFileName(path);
            ExePathLabel.Text = path;
            try
            {
                long bytes = new FileInfo(path).Length;
                ExeSizeLabel.Text = $"{bytes / 1_048_576.0:F1} MB";
            }
            catch { ExeSizeLabel.Text = ""; }

            // Auto-fill folder if not already set
            if (string.IsNullOrEmpty(FolderPathTextBox.Text))
                FolderPathTextBox.Text = Path.GetDirectoryName(path) ?? "";

            // Check run-as-admin compat flag
            RunAsAdminCheckBox.IsChecked = RegistryHelper.GetCompatFlagRunAsAdmin(path);

            ApplyState(AddGameState.Detecting);
            _ = DetectGameAsync(path);
        }

        private async Task DetectGameAsync(string exePath)
        {
            string basename = Path.GetFileNameWithoutExtension(exePath);
            string query = CleanExeName(basename);
            DetectingSubtitle.Text = $"Matching {Path.GetFileName(exePath)} against ludusavi's database…";

            List<string> found = new();
            if (_config.IsLudusaviOk)
            {
                try { found = await SearchGamesAsync(query); }
                catch (Exception ex) { App.Log($"[AddGame] Detection failed: {ex.Message}"); }
            }

            if (found.Count == 0)
            {
                // Try shorter query (first 3 words)
                var shortQuery = string.Join(" ", query.Split(' ').Take(3));
                if (shortQuery != query && _config.IsLudusaviOk)
                {
                    try { found = await SearchGamesAsync(shortQuery); }
                    catch { }
                }
            }

            if (found.Count == 0)
            {
                // Still no results — show no-match state
                ManualSearchBox.Text = query;
                ApplyState(AddGameState.NoMatch);
                _ = RunManualSearchAsync(query);
                return;
            }

            _allCandidates = BuildCandidates(found, query);

            if (_allCandidates.Count == 1 && _allCandidates[0].Confidence >= 85)
            {
                // Single high-confidence match
                _singleMatch = _allCandidates[0];
                _singleMatch.IsSelected = true;
                PopulateSingleMatch(_singleMatch);
                ApplyState(AddGameState.SingleMatch);
                _ = LoadSavePathsAsync(_singleMatch, singleMatch: true);
            }
            else
            {
                // Multiple candidates
                _candidates = _allCandidates;
                _candidates[0].IsSelected = true;
                CandidateList.ItemsSource = _candidates;
                CandidateCountLabel.Text = $"{_candidates.Count} candidate{(_candidates.Count == 1 ? "" : "s")} in ludusavi's database";
                ApplyState(AddGameState.MultiMatch);
            }

            UpdateActionButtons();
        }

        private List<LudusaviCandidate> BuildCandidates(List<string> names, string query)
        {
            var candidates = names.Select(n => new LudusaviCandidate
            {
                Name = n,
                Confidence = ComputeConfidence(query, n),
                IsBest = false,
            }).OrderByDescending(c => c.Confidence).ToList();

            if (candidates.Count > 0) candidates[0].IsBest = true;
            return candidates;
        }

        private async Task LoadSavePathsAsync(LudusaviCandidate candidate, bool singleMatch = false)
        {
            candidate.LoadingPaths = true;
            if (singleMatch) SingleMatchPathsLoading.Visibility = Visibility.Visible;

            List<string> paths = new();
            if (_config.IsLudusaviOk)
            {
                try { paths = await GetSavePathsAsync(candidate.Name); }
                catch { }
            }

            candidate.SavePaths = paths.Count > 0 ? paths : new List<string> { "No save locations found in ludusavi's database." };
            candidate.LoadingPaths = false;

            if (singleMatch)
            {
                Dispatcher.Invoke(() =>
                {
                    SingleMatchPaths.ItemsSource = candidate.SavePaths;
                    SingleMatchPathsLoading.Visibility = Visibility.Collapsed;
                    SingleMatchSubtitle.Text = $"Identified by ludusavi · {paths.Count} save {(paths.Count == 1 ? "location" : "locations")} tracked";
                });
            }
        }

        // ── State transitions ──────────────────────────────────────────────────
        private void ApplyState(AddGameState state)
        {
            _state = state;

            DropZoneBorder.Visibility = state == AddGameState.Empty ? Visibility.Visible : Visibility.Collapsed;
            ExeCardPanel.Visibility = state != AddGameState.Empty ? Visibility.Visible : Visibility.Collapsed;
            DetectingPanel.Visibility = state == AddGameState.Detecting ? Visibility.Visible : Visibility.Collapsed;
            MultiMatchPanel.Visibility = state == AddGameState.MultiMatch ? Visibility.Visible : Visibility.Collapsed;
            SingleMatchPanel.Visibility = state == AddGameState.SingleMatch ? Visibility.Visible : Visibility.Collapsed;
            NoMatchPanel.Visibility = state == AddGameState.NoMatch ? Visibility.Visible : Visibility.Collapsed;

            bool showOptions = state == AddGameState.MultiMatch
                            || state == AddGameState.SingleMatch
                            || state == AddGameState.NoMatch;
            MoreOptionsExpander.Visibility = showOptions ? Visibility.Visible : Visibility.Collapsed;

            UpdateActionButtons();
        }

        private void ResetToEmpty()
        {
            _exePath = null;
            _candidates.Clear();
            _allCandidates.Clear();
            _singleMatch = null;
            _singlePathsExpanded = false;
            _addWithoutSaveManagement = false;
            CandidateList.ItemsSource = null;
            ManualSearchBox.Text = "";
            ManualResultsList.Items.Clear();
            FolderPathTextBox.Text = "";
            RunAsAdminCheckBox.IsChecked = false;
            StatusLabel.Visibility = Visibility.Collapsed;
            StatusLabel.Text = "";
            MoreOptionsExpander.IsExpanded = false;
            ApplyState(AddGameState.Empty);
        }

        // ── Candidate interaction ──────────────────────────────────────────────
        private void CandidateRow_Click(object sender, RoutedEventArgs e)
        {
            if (sender is Button btn && btn.Tag is LudusaviCandidate c)
            {
                foreach (var x in _candidates) x.IsSelected = false;
                c.IsSelected = true;
                UpdateActionButtons();
            }
        }

        private void ExpandCandidate_Click(object sender, RoutedEventArgs e)
        {
            if (sender is Button btn && btn.Tag is LudusaviCandidate c)
            {
                c.IsExpanded = !c.IsExpanded;
                if (c.IsExpanded && c.SavePaths == null)
                    _ = LoadSavePathsAsync(c);
            }
        }

        // ── Single match interaction ───────────────────────────────────────────
        private void PopulateSingleMatch(LudusaviCandidate m)
        {
            SingleMatchName.Text = m.Name;
            SingleMatchConfidence.Text = $"{m.Confidence}% match";
            SingleMatchSubtitle.Text = $"Identified by ludusavi · loading…";
            SingleMatchPreviewButton.Content = "Preview saves";
            SingleMatchPathsPanel.Visibility = Visibility.Collapsed;
            _singlePathsExpanded = false;
        }

        private void SingleMatchPreview_Click(object sender, RoutedEventArgs e)
        {
            _singlePathsExpanded = !_singlePathsExpanded;
            SingleMatchPathsPanel.Visibility = _singlePathsExpanded ? Visibility.Visible : Visibility.Collapsed;
            SingleMatchPreviewButton.Content = _singlePathsExpanded ? "Hide saves" : "Preview saves";

            if (_singlePathsExpanded && _singleMatch?.SavePaths == null)
            {
                SingleMatchPathsLoading.Visibility = Visibility.Visible;
                _ = LoadSavePathsAsync(_singleMatch!, singleMatch: true);
            }
        }

        private void ShowAllMatches_Click(object sender, RoutedEventArgs e)
        {
            _candidates = _allCandidates;
            foreach (var c in _candidates) c.IsSelected = false;
            if (_singleMatch != null)
            {
                var sel = _candidates.FirstOrDefault(c => c.Name == _singleMatch.Name);
                if (sel != null) sel.IsSelected = true;
            }
            else if (_candidates.Count > 0) _candidates[0].IsSelected = true;

            CandidateList.ItemsSource = _candidates;
            CandidateCountLabel.Text = $"{_candidates.Count} candidate{(_candidates.Count == 1 ? "" : "s")} in ludusavi's database";
            ApplyState(AddGameState.MultiMatch);
        }

        // ── No-match / manual search ───────────────────────────────────────────
        private void SearchManually_Click(object sender, RoutedEventArgs e)
        {
            string query = _exePath != null ? CleanExeName(Path.GetFileNameWithoutExtension(_exePath)) : "";
            ManualSearchBox.Text = query;
            ApplyState(AddGameState.NoMatch);
            _ = RunManualSearchAsync(query);
        }

        private async void ManualSearch_TextChanged(object sender, TextChangedEventArgs e)
        {
            await RunManualSearchAsync(ManualSearchBox.Text);
        }

        private async Task RunManualSearchAsync(string query)
        {
            if (string.IsNullOrWhiteSpace(query))
            {
                ManualResultsList.Items.Clear();
                NoManualResultsLabel.Visibility = Visibility.Collapsed;
                return;
            }

            List<string> results = new();
            if (_config.IsLudusaviOk)
            {
                try { results = await SearchGamesAsync(query); }
                catch { }
            }

            ManualResultsList.Items.Clear();
            if (results.Count == 0)
            {
                NoManualResultsLabel.Visibility = Visibility.Visible;
            }
            else
            {
                NoManualResultsLabel.Visibility = Visibility.Collapsed;
                foreach (var g in results.Take(16)) ManualResultsList.Items.Add(g);
            }
        }

        private void ManualResults_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (ManualResultsList.SelectedItem is string name)
            {
                _addWithoutSaveManagement = false;
                // Treat the manual pick as a single confirmed match
                _singleMatch = new LudusaviCandidate { Name = name, Confidence = 100, IsBest = true, IsSelected = true };
                _allCandidates = new List<LudusaviCandidate> { _singleMatch };
                PopulateSingleMatch(_singleMatch);
                ApplyState(AddGameState.SingleMatch);
                _ = LoadSavePathsAsync(_singleMatch, singleMatch: true);
            }
            UpdateActionButtons();
        }

        private void AddWithoutSaveManagement_Click(object sender, RoutedEventArgs e)
        {
            _addWithoutSaveManagement = true;
            _singleMatch = null;
            // Pick the manual search text as the game name if nothing selected
            UpdateActionButtons();
        }

        // ── Validation / helpers ───────────────────────────────────────────────
        private string? EffectiveGameName()
        {
            if (_addWithoutSaveManagement && _exePath != null)
            {
                // Use a cleaned-up exe name if adding without save management
                string n = ManualSearchBox.Text.Trim();
                return n.Length > 0 ? n : CleanExeName(Path.GetFileNameWithoutExtension(_exePath));
            }
            if (_state == AddGameState.SingleMatch && _singleMatch != null)
                return _singleMatch.Name;
            if (_state == AddGameState.MultiMatch)
                return _candidates.FirstOrDefault(c => c.IsSelected)?.Name;
            return null;
        }

        private (string exe, string name, string safe)? ValidateFields()
        {
            string? exe = _exePath;
            string? name = EffectiveGameName();

            if (string.IsNullOrEmpty(exe))
            { ShowStatus("Please drop or browse to a game executable.", success: false); return null; }

            if (string.IsNullOrEmpty(name))
            { ShowStatus("Please select a game from the list.", success: false); return null; }

            if (!_addWithoutSaveManagement && !_config.IsLudusaviOk)
            { ShowStatus("Ludusavi not found — open Settings to configure it.", success: false); return null; }

            string safe = LauncherGenerator.MakeSafeFilename(name);
            if (string.IsNullOrEmpty(safe))
            { ShowStatus("Game name contains only invalid filename characters.", success: false); return null; }

            return (exe, name, safe);
        }

        private void UpdateActionButtons()
        {
            bool canAct = _exePath != null && EffectiveGameName() != null;
            AddToLibraryButton.IsEnabled = canAct;
            GenerateArmouryCrateButton.IsEnabled = canAct;
            AddToSteamButton.IsEnabled = canAct;
        }

        private void ApplyRunAsAdmin(GameEntry entry)
        {
            entry.RunAsAdmin = RunAsAdminCheckBox.IsChecked == true;
            if (entry.RunAsAdmin)
                RegistryHelper.SetCompatFlagRunAsAdmin(entry.ExePath);
            else
                RegistryHelper.RemoveCompatFlagRunAsAdmin(entry.ExePath);
        }

        private GameEntry BuildEntry(string exe, string name, string safe)
        {
            string folder = FolderPathTextBox.Text.Trim();
            var entry = new GameEntry
            {
                GameName = name,
                ExePath = exe,
                SafeName = safe,
                GameFolderPath = folder.Length > 0 ? folder : null,
                AddedAt = DateTime.UtcNow
            };
            ApplyRunAsAdmin(entry);
            return entry;
        }

        // ── Action buttons ────────────────────────────────────────────────────
        private void Cancel_Click(object sender, RoutedEventArgs e)
        {
            DialogResult = false;
            Close();
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

                var entry = BuildEntry(exe, name, safe);
                _library.Add(entry);

                if (_config.Data.SteamGridDbEnabled && !string.IsNullOrEmpty(_config.Data.SteamGridDbApiKey))
                    _ = Task.Run(() => FetchAndUpdateCoverArtAsync(entry));

                DialogResult = true;
                Close();
            }
            finally { SetButtonsEnabled(true); }
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
                    var existing = _library.FindByName(name);
                    entry = existing ?? BuildEntry(exe, name, safe);
                    if (existing != null)
                    {
                        existing.ExePath = exe;
                        ApplyRunAsAdmin(existing);
                        string folder = FolderPathTextBox.Text.Trim();
                        if (string.IsNullOrEmpty(existing.GameFolderPath) && folder.Length > 0)
                            existing.GameFolderPath = folder;
                    }
                    launcherExePath = await LauncherGenerator.GenerateLauncherExeAsync(entry, _config);
                    entry.LauncherExePath = launcherExePath;
                    if (existing == null) _library.Add(entry); else _library.Update(entry);
                }
                catch (Exception ex)
                { ShowStatus($"Failed to generate launcher: {ex.Message}", success: false); return; }

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
            finally { SetButtonsEnabled(true); }
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
                    var existing = _library.FindByName(name);
                    entry = existing ?? BuildEntry(exe, name, safe);
                    if (existing != null)
                    {
                        existing.ExePath = exe;
                        ApplyRunAsAdmin(existing);
                        string folder = FolderPathTextBox.Text.Trim();
                        if (string.IsNullOrEmpty(existing.GameFolderPath) && folder.Length > 0)
                            existing.GameFolderPath = folder;
                    }
                    launcherExePath = await LauncherGenerator.GenerateLauncherExeAsync(entry, _config);
                    entry.LauncherExePath = launcherExePath;
                    if (existing == null) _library.Add(entry); else _library.Update(entry);
                }
                catch (Exception ex)
                { ShowStatus($"Failed to generate launcher: {ex.Message}", success: false); return; }

                string? steamPath = await Task.Run(() => SteamIntegration.GetSteamInstallPath());
                if (steamPath == null)
                { ShowStatus("Steam installation not found. Is Steam installed?", success: false); return; }

                var users = await Task.Run(() => SteamIntegration.GetSteamUsers(steamPath));
                if (users.Count == 0)
                { ShowStatus("No Steam user profiles found. Launch Steam at least once.", success: false); return; }

                var targetUser = users.OrderByDescending(u => u.LastModified).First();

                if (SteamIntegration.IsSteamRunning())
                {
                    var answer = MessageBox.Show(
                        "Steam is currently running. Writing to shortcuts.vdf while Steam is open may cause your changes to be overwritten when Steam exits.\n\nClose Steam first, or the shortcut may not appear.\n\nContinue anyway?",
                        "Steam Is Running", MessageBoxButton.YesNo, MessageBoxImage.Warning);
                    if (answer == MessageBoxResult.No) return;
                }

                VDFParser.Models.VDFEntry[] entries;
                try { entries = await Task.Run(() => SteamIntegration.ReadShortcuts(targetUser.ShortcutsPath)); }
                catch (Exception ex)
                { ShowStatus($"Failed to read shortcuts.vdf: {ex.Message}", success: false); return; }

                string startDir = Path.GetDirectoryName(launcherExePath) ?? "";
                SteamIntegration.UpsertShortcut(ref entries, name, launcherExePath, startDir);

                try { await Task.Run(() => SteamIntegration.WriteShortcuts(targetUser.ShortcutsPath, entries)); }
                catch (Exception ex)
                { ShowStatus($"Failed to write shortcuts.vdf: {ex.Message}", success: false); return; }

                uint appId = SteamIntegration.CalculateAppId(launcherExePath, name);
                string multiUserNote = users.Count > 1 ? $" (Steam user {targetUser.UserId})" : "";
                _successWindow = new SuccessWindow(this, name, launcherExePath, SuccessMode.Steam);

                if (_config.Data.SteamGridDbEnabled && !string.IsNullOrEmpty(_config.Data.SteamGridDbApiKey))
                {
                    _successWindow.UpdateArtwork($"Fetching cover image{multiUserNote}...", "#99FFFFFF");
                    _ = Task.Run(() => FetchCoverArtForSteamAsync(name, safe, steamPath, targetUser.UserId, appId, multiUserNote));
                }
                else if (users.Count > 1)
                    _successWindow.UpdateArtwork($"Added to Steam user {targetUser.UserId}", "#4CAF50");

                _successWindow.ShowDialog();
                DialogResult = true;
                Close();
            }
            finally { SetButtonsEnabled(true); }
        }

        // ── Ludusavi helpers ──────────────────────────────────────────────────
        private async Task<List<string>> SearchGamesAsync(string query)
        {
            if (!_config.IsLudusaviOk)
                throw new InvalidOperationException("Ludusavi not configured.");

            var psi = new ProcessStartInfo
            {
                FileName = _config.Data.LudusaviPath,
                Arguments = $"find --api --fuzzy --multiple \"{query}\"",
                UseShellExecute = false,
                RedirectStandardOutput = true,
                CreateNoWindow = true
            };
            var p = new Process { StartInfo = psi };
            p.Start();
            string json = await p.StandardOutput.ReadToEndAsync();
            await p.WaitForExitAsync();
            p.Dispose();

            var response = JsonSerializer.Deserialize(json, MainSourceGenerationContext.Default.LudusaviFindResponse);
            return response?.Games?.Keys.ToList() ?? new List<string>();
        }

        private async Task<List<string>> GetSavePathsAsync(string gameName)
        {
            if (!_config.IsLudusaviOk) return new List<string>();

            var psi = new ProcessStartInfo
            {
                FileName = _config.Data.LudusaviPath,
                Arguments = $"backup --preview --api -g \"{gameName}\"",
                UseShellExecute = false,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                CreateNoWindow = true
            };
            var p = new Process { StartInfo = psi };
            p.Start();
            string json = await p.StandardOutput.ReadToEndAsync();
            await p.WaitForExitAsync();
            p.Dispose();

            // Parse ludusavi backup --preview --api output
            // The JSON has: { "games": { "GameName": { "files": { "path": {...} }, "registry": {...} } } }
            var paths = new List<string>();
            try
            {
                using var doc = JsonDocument.Parse(json);
                if (doc.RootElement.TryGetProperty("games", out var games)
                    && games.TryGetProperty(gameName, out var game))
                {
                    if (game.TryGetProperty("files", out var files))
                    {
                        foreach (var file in files.EnumerateObject())
                            paths.Add(file.Name);
                    }
                    if (game.TryGetProperty("registry", out var reg))
                    {
                        foreach (var entry in reg.EnumerateObject())
                            paths.Add(entry.Name);
                    }
                }
            }
            catch { }

            return paths;
        }

        private static string CleanExeName(string filename)
        {
            // Remove version-like suffixes (v1.2, _64, x64, etc.)
            string cleaned = Regex.Replace(filename, @"[_\-\.]", " ");
            cleaned = Regex.Replace(cleaned, @"\b(x64|x86|win64|win32|64bit|32bit|v\d[\d\.]*)\b", "", RegexOptions.IgnoreCase);
            cleaned = Regex.Replace(cleaned, @"\s{2,}", " ").Trim();
            return System.Globalization.CultureInfo.CurrentCulture.TextInfo.ToTitleCase(cleaned.ToLower());
        }

        private static int ComputeConfidence(string query, string candidate)
        {
            string q = NormStr(query);
            string c = NormStr(candidate);
            if (q == c) return 99;

            // Token overlap score
            var qTokens = new HashSet<string>(q.Split(' ', StringSplitOptions.RemoveEmptyEntries));
            var cTokens = new HashSet<string>(c.Split(' ', StringSplitOptions.RemoveEmptyEntries));
            int intersection = qTokens.Intersect(cTokens).Count();
            int union = qTokens.Union(cTokens).Count();
            double jaccard = union == 0 ? 0 : (double)intersection / union;

            // Contains bonus
            double bonus = (c.Contains(q) || q.Contains(c)) ? 0.2 : 0;
            return (int)Math.Min(98, Math.Round((jaccard + bonus) * 100));
        }

        private static string NormStr(string s) =>
            Regex.Replace(s.ToLower(), @"[^a-z0-9\s]", "").Trim();

        // ── Cover art helpers (preserved from original) ────────────────────────
        private async Task FetchAndUpdateCoverArtAsync(GameEntry entry)
        {
            try
            {
                string coversDir = Path.Combine(Config.AppDataFolder, "covers");
                Directory.CreateDirectory(coversDir);
                var sgdb = new SteamGridDbClient(_config.Data.SteamGridDbApiKey);
                var results = await sgdb.SearchGameAsync(entry.GameName);
                if (results.Count == 0) return;

                int gameId = results[0].Id;
                string destBase = Path.Combine(coversDir, entry.SafeName);
                var tPortrait = sgdb.DownloadPortraitAsync(gameId, destBase + "_p");
                var tHero = sgdb.DownloadHeroAsync(gameId, destBase + "_hero");
                await Task.WhenAll(tPortrait, tHero);

                string? imagePath = tPortrait.Result ?? await sgdb.DownloadGridImageAsync(gameId, entry.SafeName, coversDir);
                string? heroPath = tHero.Result;

                if (imagePath != null || heroPath != null)
                {
                    Application.Current?.Dispatcher.Invoke(() =>
                    {
                        if (imagePath != null) entry.CoverImagePath = imagePath;
                        if (heroPath != null) entry.HeroImagePath = heroPath;
                        _library.Update(entry);
                        if (imagePath != null) OnCoverArtFetched?.Invoke(entry, imagePath);
                    });
                }
            }
            catch (Exception ex) { App.Log($"[AddGameWindow] Cover art error '{entry.GameName}': {ex.Message}"); }
        }

        private async Task FetchCoverArtForACAsync(string gameName, string safeName)
        {
            string coversDir = Path.Combine(Config.AppDataFolder, "covers");
            try
            {
                var sgdb = new SteamGridDbClient(_config.Data.SteamGridDbApiKey);
                var results = await sgdb.SearchGameAsync(gameName);
                if (results.Count == 0)
                { Dispatcher.Invoke(() => _successWindow?.UpdateArtwork("⚠ Game not found on SteamGridDB", "#FFC107")); return; }

                int gameId = results[0].Id;
                string? imgPath = await sgdb.DownloadGridImageAsync(gameId, safeName, coversDir);
                if (imgPath == null)
                    Dispatcher.Invoke(() => _successWindow?.UpdateArtwork("⚠ No horizontal grid images found on SteamGridDB", "#FFC107"));
                else
                    Dispatcher.Invoke(() => _successWindow?.UpdateArtwork($"Cover art: {imgPath}", "#4CAF50"));
            }
            catch (Exception ex)
            {
                App.Log($"[AddGameWindow] AC cover art error '{gameName}': {ex.Message}");
                Dispatcher.Invoke(() => _successWindow?.UpdateArtwork($"⚠ Artwork error: {ex.Message}", "#FFC107"));
            }
        }

        private async Task FetchCoverArtForSteamAsync(string gameName, string safeName, string steamPath, string userId, uint appId, string multiUserNote)
        {
            string coversDir = Path.Combine(Config.AppDataFolder, "covers");
            string gridDir = Path.Combine(steamPath, "userdata", userId, "config", "grid");
            try
            {
                var sgdb = new SteamGridDbClient(_config.Data.SteamGridDbApiKey);
                var results = await sgdb.SearchGameAsync(gameName);
                if (results.Count == 0)
                { Dispatcher.Invoke(() => _successWindow?.UpdateArtwork("⚠ Game not found on SteamGridDB", "#FFC107")); return; }

                int gameId = results[0].Id;
                Directory.CreateDirectory(coversDir);
                Directory.CreateDirectory(gridDir);
                string gridBase = Path.Combine(gridDir, appId.ToString());

                var tHoriz = sgdb.DownloadGridImageAsync(gameId, safeName, coversDir);
                var tPortrait = sgdb.DownloadPortraitAsync(gameId, gridBase + "p");
                var tHero = sgdb.DownloadHeroAsync(gameId, gridBase + "_hero");
                var tLogo = sgdb.DownloadLogoAsync(gameId, gridBase + "_logo");
                await Task.WhenAll(tHoriz, tPortrait, tHero, tLogo);

                SteamIntegration.CopyGridImage(tHoriz.Result, steamPath, userId, appId, suffix: "");

                int copied = new[] { tHoriz.Result, tPortrait.Result, tHero.Result, tLogo.Result }.Count(x => x != null);
                if (copied == 0)
                    Dispatcher.Invoke(() => _successWindow?.UpdateArtwork($"⚠ No images found on SteamGridDB{multiUserNote}", "#FFC107"));
                else
                {
                    string detail = string.Join(", ", new[] {
                        tHoriz.Result   != null ? "grid"     : null,
                        tPortrait.Result!= null ? "portrait" : null,
                        tHero.Result    != null ? "hero"     : null,
                        tLogo.Result    != null ? "logo"     : null,
                    }.Where(s => s != null));
                    Dispatcher.Invoke(() => _successWindow?.UpdateArtwork($"Art added to Steam ({detail}){multiUserNote}", "#4CAF50"));
                }
            }
            catch (Exception ex)
            {
                App.Log($"[AddGameWindow] Steam cover art error '{gameName}': {ex.Message}");
                Dispatcher.Invoke(() => _successWindow?.UpdateArtwork($"⚠ Artwork error: {ex.Message}", "#FFC107"));
            }
        }

        // ── UI helpers ────────────────────────────────────────────────────────
        private void SetButtonsEnabled(bool enabled)
        {
            AddToLibraryButton.IsEnabled = enabled && EffectiveGameName() != null;
            GenerateArmouryCrateButton.IsEnabled = enabled && EffectiveGameName() != null;
            AddToSteamButton.IsEnabled = enabled && EffectiveGameName() != null;
        }

        private void ShowStatus(string message, bool success)
        {
            StatusLabel.Text = message;
            StatusLabel.Foreground = success
                ? new SolidColorBrush(Color.FromRgb(0x4C, 0xAF, 0x50))
                : new SolidColorBrush(Color.FromRgb(0xF4, 0x43, 0x36));
            StatusLabel.Visibility = string.IsNullOrEmpty(message) ? Visibility.Collapsed : Visibility.Visible;
        }
    }
}
