using System;
using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using Microsoft.Win32;
using Wpf.Ui.Appearance;

namespace LudusaviWrap
{
    public partial class SetupWindow : Wpf.Ui.Controls.FluentWindow
    {
        private readonly Config _config;
        private readonly bool _isFirstRun;
        private bool _themeComboInitialized = false;

        public SetupWindow(Config config, bool isFirstRun = false)
        {
            // Must be called before InitializeComponent so the window backdrop and
            // DWM dark-mode attribute are set before the visual tree is rendered.
            SystemThemeWatcher.Watch(this);

            InitializeComponent();

            // Re-apply the configured app theme so dialogs follow the user's preference
            // rather than the Windows system theme (they may differ).
            ThemeManager.ApplyTheme(config.Data.Theme);
            _config = config;
            _isFirstRun = isFirstRun;

            // Load values from configuration
            LudusaviPathTextBox.Text = _config.Data.LudusaviPath;
            SgdbSwitch.IsChecked = _config.Data.SteamGridDbEnabled;
            ApiKeyTextBox.Text = _config.Data.SteamGridDbApiKey;

            SyncSwitch.IsChecked    = _config.Data.SyncServerEnabled;
            SyncUrlTextBox.Text     = _config.Data.SyncServerUrl;
            SyncApiKeyBox.Password  = _config.Data.SyncServerApiKey;
            DeviceNameTextBox.Text  = _config.Data.DeviceName;

            LanSwitch.IsChecked        = _config.Data.LanShareEnabled;
            LanPortTextBox.Text        = _config.Data.LanSharePort.ToString();
            LanInstallDirTextBox.Text  = _config.Data.LanInstallDir;

            // Populate theme ComboBox
            SelectThemeComboItem(_config.Data.Theme);
            _themeComboInitialized = true;

            // Update UI state based on switches
            UpdateSgdbUiState();
            UpdateSyncUiState();
            UpdateLanUiState();
        }

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
            // Fallback to system
            ThemeComboBox.SelectedIndex = 0;
        }

        private void ThemeComboBox_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            // Guard against firing during InitializeComponent
            if (!_themeComboInitialized) return;

            if (ThemeComboBox.SelectedItem is ComboBoxItem selected &&
                selected.Tag?.ToString() is string tag)
            {
                // Live preview — apply immediately without saving
                ThemeManager.ApplyTheme(tag);
            }
        }

        private void SgdbSwitch_Changed(object sender, RoutedEventArgs e)
        {
            UpdateSgdbUiState();
        }

        private void UpdateSgdbUiState()
        {
            if (ApiKeyGrid != null && ApiKeyTextBox != null)
            {
                bool enabled = SgdbSwitch.IsChecked ?? false;
                ApiKeyTextBox.IsEnabled = enabled;
            }
        }

        private void SyncSwitch_Changed(object sender, RoutedEventArgs e)
        {
            UpdateSyncUiState();
        }

        private void UpdateSyncUiState()
        {
            if (SyncFieldsGrid == null) return;
            bool enabled = SyncSwitch.IsChecked ?? false;
            SyncFieldsGrid.IsEnabled = enabled;
        }

        private void LanSwitch_Changed(object sender, RoutedEventArgs e)
        {
            UpdateLanUiState();
        }

        private void UpdateLanUiState()
        {
            if (LanFieldsGrid == null) return;
            LanFieldsGrid.IsEnabled = LanSwitch.IsChecked ?? false;
        }

        private void BrowseLanInstallDir_Click(object sender, RoutedEventArgs e)
        {
            var dialog = new OpenFolderDialog
            {
                Title = "Select default folder for LAN game downloads",
                Multiselect = false
            };
            if (dialog.ShowDialog() == true)
                LanInstallDirTextBox.Text = dialog.FolderName;
        }

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
            }
        }

        private void GetKey_Click(object sender, RoutedEventArgs e)
        {
            try
            {
                Process.Start(new ProcessStartInfo
                {
                    FileName = "https://www.steamgriddb.com/profile/preferences/api",
                    UseShellExecute = true
                });
            }
            catch (Exception ex)
            {
                ShowError($"Could not open website: {ex.Message}");
            }
        }

        private void Save_Click(object sender, RoutedEventArgs e)
        {
            string path = LudusaviPathTextBox.Text.Trim();
            bool sgdbEnabled = SgdbSwitch.IsChecked ?? false;
            string apiKey = ApiKeyTextBox.Text.Trim();

            if (string.IsNullOrEmpty(path) || !File.Exists(path))
            {
                ShowError("Please select a valid ludusavi.exe file.");
                return;
            }

            if (sgdbEnabled && string.IsNullOrEmpty(apiKey))
            {
                ShowError("API key is required when SteamGridDB is enabled.");
                return;
            }

            // Persist theme selection
            string themeValue = "system";
            if (ThemeComboBox.SelectedItem is ComboBoxItem selectedTheme &&
                selectedTheme.Tag?.ToString() is string tag)
            {
                themeValue = tag;
            }

            // Validate LAN port
            if (!int.TryParse(LanPortTextBox.Text.Trim(), out int lanPort) || lanPort < 1024 || lanPort > 65534)
            {
                ShowError("LAN port must be a number between 1024 and 65534.");
                return;
            }

            // Save back to configuration
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
            _config.Save();

            DialogResult = true;
            Close();
        }

        private void Cancel_Click(object sender, RoutedEventArgs e)
        {
            // Revert live preview to saved preference
            ThemeManager.ApplyTheme(_config.Data.Theme);
            DialogResult = false;
            Close();
        }

        private void ShowError(string message)
        {
            ErrorLabel.Text = message;
            ErrorLabel.Visibility = Visibility.Visible;
        }
    }
}
