using System;
using System.Collections.ObjectModel;
using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using Microsoft.Win32;
using Wpf.Ui.Appearance;

namespace LudusaviWrap
{
    public partial class SetupWindow : Wpf.Ui.Controls.FluentWindow
    {
        private readonly Config _config;
        private readonly bool _isFirstRun;
        private bool _themeComboInitialized = false;
        private bool _dirty = false;
        private bool _closeConfirmed = false;
        private readonly ObservableCollection<string> _sources = new();

        public SetupWindow(Config config, bool isFirstRun = false)
        {
            SystemThemeWatcher.Watch(this);
            InitializeComponent();
            ThemeManager.ApplyTheme(config.Data.Theme);
            _config = config;
            _isFirstRun = isFirstRun;

            // Populate fields
            LudusaviPathTextBox.Text      = _config.Data.LudusaviPath;
            SgdbSwitch.IsChecked          = _config.Data.SteamGridDbEnabled;
            ApiKeyTextBox.Text            = _config.Data.SteamGridDbApiKey;

            SyncSwitch.IsChecked          = _config.Data.SyncServerEnabled;
            SyncUrlTextBox.Text           = _config.Data.SyncServerUrl;
            SyncApiKeyBox.Password        = _config.Data.SyncServerApiKey;
            DeviceNameTextBox.Text        = _config.Data.DeviceName;

            LanSwitch.IsChecked           = _config.Data.LanShareEnabled;
            LanPortTextBox.Text           = _config.Data.LanSharePort.ToString();
            LanInstallDirTextBox.Text     = _config.Data.LanInstallDir;

            TorBoxSwitch.IsChecked        = _config.Data.TorBoxEnabled;
            TorBoxApiKeyBox.Password      = _config.Data.TorBoxApiKey;
            TorBoxDownloadDirTextBox.Text = _config.Data.DownloadDir;

            foreach (var url in _config.Data.DownloadSources)
                _sources.Add(url);
            SourcesListBox.ItemsSource = _sources;
            UpdateSourcesCount();

            SelectThemeComboItem(_config.Data.Theme);
            _themeComboInitialized = true;

            AboutVersionText.Text = $"Spool v{GetAppVersion()} · Up to date";

            // Expand cards whose services are already enabled
            UpdateSgdbExpander();
            UpdateSyncExpander();
            UpdateLanExpander();
            UpdateTorBoxExpander();

            // Status pills
            UpdateLudusaviPill();
            UpdateSgdbPill();
            UpdateLanPill();
            UpdateSyncPill();
            UpdateTorBoxPill();

            // Start on Cloud sync section (mirrors the design mock default)
            NavList.SelectedItem = NavSync;

            SetDirty(false);

            if (_config.Data.SyncServerEnabled && !string.IsNullOrEmpty(_config.Data.SyncServerUrl))
                _ = CheckAndShowServerVersionAsync(_config.Data.SyncServerUrl);
        }

        // ─────────────────────────────────────────────────────────────────────
        // Navigation
        // ─────────────────────────────────────────────────────────────────────

        private void NavList_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (NavList.SelectedItem is not ListBoxItem item) return;
            var tag = item.Tag?.ToString() ?? "";
            ShowSection(tag);
            UpdateNavPills(tag);
        }

        private void SearchBox_TextChanged(object sender, TextChangedEventArgs e)
        {
            string q = SearchBox.Text.Trim().ToLowerInvariant();
            // Show/hide nav items based on label match
            foreach (ListBoxItem item in NavList.Items)
            {
                if (item.Content is Grid g &&
                    g.Children.Count > 2 &&
                    g.Children[2] is TextBlock tb)
                {
                    item.Visibility = string.IsNullOrEmpty(q) || tb.Text.ToLowerInvariant().Contains(q)
                        ? Visibility.Visible
                        : Visibility.Collapsed;
                }
            }
        }

        private void ShowSection(string tag)
        {
            SectionGeneral.Visibility   = tag == "general"   ? Visibility.Visible : Visibility.Collapsed;
            SectionArtwork.Visibility   = tag == "artwork"   ? Visibility.Visible : Visibility.Collapsed;
            SectionSources.Visibility   = tag == "sources"   ? Visibility.Visible : Visibility.Collapsed;
            SectionLan.Visibility       = tag == "lan"       ? Visibility.Visible : Visibility.Collapsed;
            SectionSync.Visibility      = tag == "sync"      ? Visibility.Visible : Visibility.Collapsed;
            SectionDownloads.Visibility = tag == "downloads" ? Visibility.Visible : Visibility.Collapsed;
        }

        private void UpdateNavPills(string activeTag)
        {
            SetNavPill(NavGeneralPill,   "general",   activeTag);
            SetNavPill(NavArtworkPill,   "artwork",   activeTag);
            SetNavPill(NavSourcesPill,   "sources",   activeTag);
            SetNavPill(NavLanPill,       "lan",       activeTag);
            SetNavPill(NavSyncPill,      "sync",      activeTag);
            SetNavPill(NavDownloadsPill, "downloads", activeTag);
        }

        private static void SetNavPill(Border? pill, string tag, string activeTag)
        {
            if (pill == null) return;
            pill.Visibility = tag == activeTag ? Visibility.Visible : Visibility.Hidden;
        }

        // ─────────────────────────────────────────────────────────────────────
        // Expandable cards
        // ─────────────────────────────────────────────────────────────────────

        private void SgdbCard_Click(object sender, System.Windows.Input.MouseButtonEventArgs e)
        {
            bool nowExpanded = SgdbBody.Visibility != Visibility.Visible;
            SgdbBody.Visibility = nowExpanded ? Visibility.Visible : Visibility.Collapsed;
        }

        private void SgdbSwitch_PreviewClick(object sender, System.Windows.Input.MouseButtonEventArgs e)
        {
            // Prevent the card row click handler from toggling expansion when toggling the switch
            e.Handled = false;
        }

        private void LanCard_Click(object sender, System.Windows.Input.MouseButtonEventArgs e)
        {
            bool nowExpanded = LanBody.Visibility != Visibility.Visible;
            LanBody.Visibility = nowExpanded ? Visibility.Visible : Visibility.Collapsed;
        }

        private void LanSwitch_PreviewClick(object sender, System.Windows.Input.MouseButtonEventArgs e)
        {
            e.Handled = false;
        }

        private void SyncCard_Click(object sender, System.Windows.Input.MouseButtonEventArgs e)
        {
            bool nowExpanded = SyncBody.Visibility != Visibility.Visible;
            SyncBody.Visibility = nowExpanded ? Visibility.Visible : Visibility.Collapsed;
        }

        private void SyncSwitch_PreviewClick(object sender, System.Windows.Input.MouseButtonEventArgs e)
        {
            e.Handled = false;
        }

        private void TorBoxCard_Click(object sender, System.Windows.Input.MouseButtonEventArgs e)
        {
            bool nowExpanded = TorBoxBody.Visibility != Visibility.Visible;
            TorBoxBody.Visibility = nowExpanded ? Visibility.Visible : Visibility.Collapsed;
        }

        private void TorBoxSwitch_PreviewClick(object sender, System.Windows.Input.MouseButtonEventArgs e)
        {
            e.Handled = false;
        }

        // Sync expander open state with switch state on load
        private void UpdateSgdbExpander()
        {
            if (SgdbBody == null) return;
            SgdbBody.Visibility = (SgdbSwitch.IsChecked ?? false) ? Visibility.Visible : Visibility.Collapsed;
        }

        private void UpdateSyncExpander()
        {
            if (SyncBody == null) return;
            SyncBody.Visibility = (SyncSwitch.IsChecked ?? false) ? Visibility.Visible : Visibility.Collapsed;
        }

        private void UpdateLanExpander()
        {
            if (LanBody == null) return;
            LanBody.Visibility = (LanSwitch.IsChecked ?? false) ? Visibility.Visible : Visibility.Collapsed;
        }

        private void UpdateTorBoxExpander()
        {
            if (TorBoxBody == null) return;
            TorBoxBody.Visibility = (TorBoxSwitch.IsChecked ?? false) ? Visibility.Visible : Visibility.Collapsed;
        }

        // ─────────────────────────────────────────────────────────────────────
        // Status pills
        // ─────────────────────────────────────────────────────────────────────

        private void UpdateLudusaviPill()
        {
            bool found = !string.IsNullOrEmpty(LudusaviPathTextBox.Text) &&
                         File.Exists(LudusaviPathTextBox.Text.Trim());
            LudusaviPill.Visibility = found ? Visibility.Visible : Visibility.Collapsed;
        }

        private void UpdateSgdbPill()
        {
            bool show = (SgdbSwitch.IsChecked ?? false) && !string.IsNullOrEmpty(ApiKeyTextBox.Text);
            SgdbPill.Visibility = show ? Visibility.Visible : Visibility.Collapsed;
        }

        private void UpdateLanPill()
        {
            bool show = LanSwitch.IsChecked ?? false;
            LanPill.Visibility = show ? Visibility.Visible : Visibility.Collapsed;
            if (show)
                LanPillText.Text = $"Listening :{LanPortTextBox.Text.Trim()}";
        }

        private void UpdateSyncPill()
        {
            bool show = (SyncSwitch.IsChecked ?? false) && !string.IsNullOrEmpty(SyncUrlTextBox.Text);
            SyncPill.Visibility = show ? Visibility.Visible : Visibility.Collapsed;
        }

        private void UpdateTorBoxPill()
        {
            bool show = (TorBoxSwitch.IsChecked ?? false) && TorBoxApiKeyBox.Password.Length > 0;
            TorBoxPill.Visibility = show ? Visibility.Visible : Visibility.Collapsed;
        }

        // ─────────────────────────────────────────────────────────────────────
        // Dirty state
        // ─────────────────────────────────────────────────────────────────────

        private void SetDirty(bool dirty)
        {
            _dirty = dirty;
            if (dirty)
            {
                DirtyDot.Fill = new SolidColorBrush(Color.FromRgb(0xFF, 0xC2, 0x78));
                DirtyLabel.Text = "Unsaved changes";
                DirtyLabel.Foreground = new SolidColorBrush(Color.FromRgb(0xFF, 0xC2, 0x78));
            }
            else
            {
                DirtyDot.Fill = new SolidColorBrush(Color.FromRgb(0x7E, 0xE2, 0xA4));
                DirtyLabel.Text = "All changes saved";
                DirtyLabel.SetResourceReference(System.Windows.Controls.TextBlock.ForegroundProperty,
                    "TextFillColorTertiaryBrush");
            }
        }

        // ─────────────────────────────────────────────────────────────────────
        // Switch handlers
        // ─────────────────────────────────────────────────────────────────────

        private void SgdbSwitch_Changed(object sender, RoutedEventArgs e)
        {
            UpdateSgdbPill();
            SetDirty(true);
        }

        private void SyncSwitch_Changed(object sender, RoutedEventArgs e)
        {
            UpdateSyncPill();
            SetDirty(true);
        }

        private void LanSwitch_Changed(object sender, RoutedEventArgs e)
        {
            UpdateLanPill();
            SetDirty(true);
        }

        private void TorBoxSwitch_Changed(object sender, RoutedEventArgs e)
        {
            UpdateTorBoxPill();
            SetDirty(true);
        }

        private void LudusaviPath_TextChanged(object sender, TextChangedEventArgs e)
        {
            UpdateLudusaviPill();
            SetDirty(true);
        }

        // ─────────────────────────────────────────────────────────────────────
        // Theme
        // ─────────────────────────────────────────────────────────────────────

        private void SelectThemeComboItem(string theme)
        {
            foreach (ComboBoxItem item in ThemeComboBox.Items)
            {
                if (item.Tag?.ToString() == theme)
                {
                    ThemeComboBox.SelectedItem = item;
                    return;
                }
            }
            ThemeComboBox.SelectedIndex = 0;
        }

        private void ThemeComboBox_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            if (!_themeComboInitialized) return;
            if (ThemeComboBox.SelectedItem is ComboBoxItem selected &&
                selected.Tag?.ToString() is string tag)
            {
                ThemeManager.ApplyTheme(tag);
                SetDirty(true);
            }
        }

        // ─────────────────────────────────────────────────────────────────────
        // Sources
        // ─────────────────────────────────────────────────────────────────────

        private void AddSource_Click(object sender, RoutedEventArgs e)
        {
            string url = NewSourceUrlTextBox.Text.Trim();
            if (string.IsNullOrEmpty(url)) return;
            if (!Uri.TryCreate(url, UriKind.Absolute, out _))
            {
                ShowError("Please enter a valid URL.");
                return;
            }
            if (!_sources.Contains(url))
            {
                _sources.Add(url);
                SetDirty(true);
            }
            NewSourceUrlTextBox.Text = "";
            ErrorLabel.Visibility = Visibility.Collapsed;
            UpdateSourcesCount();
        }

        private void RemoveSource_Click(object sender, RoutedEventArgs e)
        {
            string? url = null;
            if (sender is Button btn && btn.Tag is string tag)
                url = tag;
            else if (SourcesListBox.SelectedItem is string selected)
                url = selected;

            if (url != null)
            {
                _sources.Remove(url);
                SetDirty(true);
                UpdateSourcesCount();
            }
        }

        private void UpdateSourcesCount()
        {
            SourcesCountLabel.Text = $"{_sources.Count} source{(_sources.Count == 1 ? "" : "s")} configured";
        }

        // ─────────────────────────────────────────────────────────────────────
        // Browse dialogs
        // ─────────────────────────────────────────────────────────────────────

        private void BrowseLudusavi_Click(object sender, RoutedEventArgs e)
        {
            var dialog = new OpenFileDialog
            {
                Title = "Select ludusavi.exe",
                Filter = "ludusavi.exe|ludusavi.exe|Executables (*.exe)|*.exe",
                RestoreDirectory = true
            };
            if (dialog.ShowDialog() == true)
            {
                LudusaviPathTextBox.Text = dialog.FileName;
                SetDirty(true);
            }
        }

        private void BrowseLanInstallDir_Click(object sender, RoutedEventArgs e)
        {
            var dialog = new OpenFolderDialog
            {
                Title = "Select default folder for LAN game downloads",
                Multiselect = false
            };
            if (dialog.ShowDialog() == true)
            {
                LanInstallDirTextBox.Text = dialog.FolderName;
                SetDirty(true);
            }
        }

        private void BrowseTorBoxDir_Click(object sender, RoutedEventArgs e)
        {
            var dialog = new OpenFolderDialog
            {
                Title = "Select default folder for TorBox downloads",
                Multiselect = false
            };
            if (dialog.ShowDialog() == true)
            {
                TorBoxDownloadDirTextBox.Text = dialog.FolderName;
                SetDirty(true);
            }
        }

        // ─────────────────────────────────────────────────────────────────────
        // External links
        // ─────────────────────────────────────────────────────────────────────

        private void GetKey_Click(object sender, RoutedEventArgs e)
            => OpenUrl("https://www.steamgriddb.com/profile/preferences/api");

        private void TorBoxGetKey_Click(object sender, RoutedEventArgs e)
            => OpenUrl("https://torbox.app/settings");

        private void ReleaseNotes_Click(object sender, RoutedEventArgs e)
            => OpenUrl("https://github.com/aidankinzett/Spool/releases");

        private void OpenUrl(string url)
        {
            try
            {
                Process.Start(new ProcessStartInfo { FileName = url, UseShellExecute = true });
            }
            catch (Exception ex)
            {
                ShowError($"Could not open website: {ex.Message}");
            }
        }

        // ─────────────────────────────────────────────────────────────────────
        // Sync — Scan LAN + server version
        // ─────────────────────────────────────────────────────────────────────

        private async void ScanLan_Click(object sender, RoutedEventArgs e)
        {
            ScanLanButton.IsEnabled = false;
            ScanLanButton.Content = "Scanning…";
            ErrorLabel.Visibility = Visibility.Collapsed;
            ServerVersionLabel.Visibility = Visibility.Collapsed;

            try
            {
                var found = await PlayStateLockClient.ScanLanAsync();
                if (found.Count == 0)
                {
                    ShowError("No sync server found on the local network.");
                }
                else
                {
                    SyncUrlTextBox.Text = found[0];
                    SetDirty(true);
                    if (found.Count > 1)
                        ShowError($"Found {found.Count} servers — selected the first one.");
                    await CheckAndShowServerVersionAsync(found[0]);
                }
            }
            finally
            {
                ScanLanButton.IsEnabled = true;
                ScanLanButton.Content = "Scan LAN";
            }
        }

        private async Task CheckAndShowServerVersionAsync(string serverUrl)
        {
            var health = await PlayStateLockClient.CheckHealthAsync(serverUrl);
            if (health == null)
            {
                ServerVersionLabel.Text = "Server unreachable";
                ServerVersionLabel.Foreground = Brushes.Gray;
                ServerVersionLabel.Visibility = Visibility.Visible;
                return;
            }

            string serverVer = health.Version ?? "unknown";
            string appVer = GetAppVersion();

            if (serverVer == "dev" || appVer == "1.0.0")
            {
                ServerVersionLabel.Text = $"Server version: {serverVer}";
                ServerVersionLabel.Foreground = Brushes.Gray;
                SyncPillText.Text = $"{serverVer} · connected";
            }
            else if (serverVer == appVer)
            {
                ServerVersionLabel.Text = $"Server v{serverVer} — up to date";
                ServerVersionLabel.Foreground = new SolidColorBrush(Color.FromRgb(0x4C, 0xAF, 0x50));
                SyncPillText.Text = $"v{serverVer} · connected";
            }
            else
            {
                ServerVersionLabel.Text = $"Server v{serverVer} — app is v{appVer}, consider updating the server";
                ServerVersionLabel.Foreground = new SolidColorBrush(Color.FromRgb(0xFF, 0xA0, 0x26));
                SyncPillText.Text = $"v{serverVer} · outdated";
            }

            ServerVersionLabel.Visibility = Visibility.Visible;
            SyncApiKeyHelper.Text = $"Server {serverVer} — up to date.";
            UpdateSyncPill();
        }

        // ─────────────────────────────────────────────────────────────────────
        // Register panel
        // ─────────────────────────────────────────────────────────────────────

        private void RegisterPanel_Toggle(object sender, RoutedEventArgs e)
        {
            bool visible = RegisterPanel.Visibility == Visibility.Visible;
            RegisterPanel.Visibility = visible ? Visibility.Collapsed : Visibility.Visible;
            if (!visible && string.IsNullOrEmpty(RegisterUsernameBox.Text))
                RegisterUsernameBox.Text = _config.Data.DeviceName;
        }

        private void RegisterCancel_Click(object sender, RoutedEventArgs e)
        {
            RegisterPanel.Visibility = Visibility.Collapsed;
            RegisterErrorLabel.Visibility = Visibility.Collapsed;
        }

        private async void Register_Click(object sender, RoutedEventArgs e)
        {
            string url = SyncUrlTextBox.Text.Trim();
            string adminSecret = RegisterAdminSecretBox.Password.Trim();
            string username = RegisterUsernameBox.Text.Trim();

            if (string.IsNullOrEmpty(url))
            {
                ShowRegisterError("Enter the server URL first.");
                return;
            }
            if (string.IsNullOrEmpty(adminSecret))
            {
                ShowRegisterError("Admin secret is required.");
                return;
            }
            if (string.IsNullOrEmpty(username))
            {
                ShowRegisterError("Username is required.");
                return;
            }

            RegisterErrorLabel.Visibility = Visibility.Collapsed;
            RegisterButton.IsEnabled = false;
            RegisterButton.Content = "Registering…";

            try
            {
                var (apiKey, error) = await PlayStateLockClient.RegisterAsync(url, adminSecret, username);
                if (apiKey != null)
                {
                    SyncApiKeyBox.Password = apiKey;
                    RegisterAdminSecretBox.Clear();
                    SetDirty(true);

                    RegisterErrorLabel.Text = "Account created — API key filled in.";
                    RegisterErrorLabel.Foreground = new SolidColorBrush(Color.FromRgb(0x4C, 0xAF, 0x50));
                    RegisterErrorLabel.Visibility = Visibility.Visible;

                    await Task.Delay(1800);
                    RegisterPanel.Visibility = Visibility.Collapsed;
                    RegisterErrorLabel.Foreground = new SolidColorBrush(Color.FromRgb(0xFF, 0x8A, 0x8A));
                }
                else
                {
                    ShowRegisterError(error ?? "Registration failed.");
                }
            }
            finally
            {
                RegisterButton.IsEnabled = true;
                RegisterButton.Content = "Register";
            }
        }

        private void ShowRegisterError(string message)
        {
            RegisterErrorLabel.Text = message;
            RegisterErrorLabel.Foreground = new SolidColorBrush(Color.FromRgb(0xFF, 0x8A, 0x8A));
            RegisterErrorLabel.Visibility = Visibility.Visible;
        }

        // ─────────────────────────────────────────────────────────────────────
        // Save / Cancel
        // ─────────────────────────────────────────────────────────────────────

        private void Save_Click(object sender, RoutedEventArgs e)
        {
            string path = LudusaviPathTextBox.Text.Trim();
            bool sgdbEnabled = SgdbSwitch.IsChecked ?? false;
            string apiKey = ApiKeyTextBox.Text.Trim();

            if (string.IsNullOrEmpty(path) || !File.Exists(path))
            {
                ShowError("Please select a valid ludusavi.exe file.");
                NavList.SelectedItem = NavGeneral;
                return;
            }

            if (sgdbEnabled && string.IsNullOrEmpty(apiKey))
            {
                ShowError("API key is required when SteamGridDB is enabled.");
                NavList.SelectedItem = NavArtwork;
                return;
            }

            string themeValue = "system";
            if (ThemeComboBox.SelectedItem is ComboBoxItem selectedTheme &&
                selectedTheme.Tag?.ToString() is string tag)
                themeValue = tag;

            string lanPortText = LanPortTextBox.Text.Trim();
            if (!int.TryParse(lanPortText, out int lanPort) || lanPort < 1024 || lanPort > 65534)
            {
                ShowError("LAN port must be a number between 1024 and 65534.");
                NavList.SelectedItem = NavLan;
                return;
            }

            _config.Data.LudusaviPath       = path;
            _config.Data.SteamGridDbEnabled = sgdbEnabled;
            _config.Data.SteamGridDbApiKey  = apiKey;
            _config.Data.Theme              = themeValue;
            _config.Data.SyncServerEnabled  = SyncSwitch.IsChecked ?? false;
            _config.Data.SyncServerUrl      = SyncUrlTextBox.Text.Trim();
            _config.Data.SyncServerApiKey   = SyncApiKeyBox.Password.Trim();
            if (!string.IsNullOrWhiteSpace(DeviceNameTextBox.Text))
                _config.Data.DeviceName     = DeviceNameTextBox.Text.Trim();
            _config.Data.LanShareEnabled    = LanSwitch.IsChecked ?? false;
            _config.Data.LanSharePort       = lanPort;
            _config.Data.LanInstallDir      = LanInstallDirTextBox.Text.Trim();
            _config.Data.TorBoxEnabled      = TorBoxSwitch.IsChecked ?? false;
            _config.Data.TorBoxApiKey       = TorBoxApiKeyBox.Password.Trim();
            _config.Data.DownloadDir        = TorBoxDownloadDirTextBox.Text.Trim();
            _config.Data.DownloadSources    = new System.Collections.Generic.List<string>(_sources);
            _config.Save();

            SetDirty(false);
            _closeConfirmed = true;
            DialogResult = true;
        }

        private void Cancel_Click(object sender, RoutedEventArgs e)
        {
            ThemeManager.ApplyTheme(_config.Data.Theme);
            _closeConfirmed = true;
            DialogResult = false;
        }

        private void Window_Closing(object sender, System.ComponentModel.CancelEventArgs e)
        {
            if (_closeConfirmed || !_dirty) return;

            var result = MessageBox.Show(
                "You have unsaved changes. Save before closing?",
                "Unsaved changes",
                MessageBoxButton.YesNoCancel,
                MessageBoxImage.Warning);

            if (result == MessageBoxResult.Yes)
            {
                // Cancel the close, then attempt save after the handler returns.
                // Deferring via BeginInvoke avoids calling DialogResult/Close
                // while the window is still mid-close-event.
                e.Cancel = true;
                Dispatcher.BeginInvoke(new Action(() => Save_Click(this, new RoutedEventArgs())));
            }
            else if (result == MessageBoxResult.No)
            {
                // Discard — revert live theme preview
                ThemeManager.ApplyTheme(_config.Data.Theme);
                _closeConfirmed = true;
                DialogResult = false;
            }
            else
            {
                // Cancel — stay in the window
                e.Cancel = true;
            }
        }

        // ─────────────────────────────────────────────────────────────────────
        // Helpers
        // ─────────────────────────────────────────────────────────────────────

        private void ShowError(string message)
        {
            ErrorLabel.Text = message;
            ErrorLabel.Visibility = Visibility.Visible;
        }

        private static string GetAppVersion() =>
            typeof(SetupWindow).Assembly.GetName().Version?.ToString(3) ?? "unknown";
    }
}
