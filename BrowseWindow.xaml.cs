using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Data;
using System.Windows.Input;
using System.Windows.Threading;
using Wpf.Ui.Appearance;

namespace LudusaviWrap
{
    public partial class BrowseWindow : Wpf.Ui.Controls.FluentWindow
    {
        private readonly Config _config;
        private List<HydraDownloadEntry> _allEntries = new();
        private ICollectionView? _view;
        private readonly DispatcherTimer _searchTimer;
        private string _searchText = "";
        private string? _currentSortProperty;
        private ListSortDirection _currentSortDir = ListSortDirection.Ascending;
        private GridViewColumnHeader? _lastSortedHeader;

        private static readonly Dictionary<string, string> ColumnToProperty = new(StringComparer.Ordinal)
        {
            ["Title"] = "Title",
            ["Size"] = "FileSize",
            ["Date"] = "UploadDateParsed",
            ["Source"] = "SourceName",
        };

        public HydraDownloadEntry? SelectedDownload { get; private set; }

        public BrowseWindow(Config config)
        {
            SystemThemeWatcher.Watch(this);
            InitializeComponent();
            ThemeManager.ApplyTheme(config.Data.Theme);
            _config = config;

            _searchTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(300) };
            _searchTimer.Tick += (_, _) =>
            {
                _searchTimer.Stop();
                ApplyFilter();
            };

            Loaded += async (_, _) => await LoadSourcesAsync();
        }

        private async System.Threading.Tasks.Task LoadSourcesAsync()
        {
            var sources = _config.Data.DownloadSources;
            if (sources.Count == 0)
            {
                ResultCountText.Text = "No download sources configured — add some in Settings.";
                ResultCountText.Visibility = Visibility.Visible;
                return;
            }

            LoadingPanel.Visibility = Visibility.Visible;
            ResultCountText.Visibility = Visibility.Collapsed;
            RefreshButton.IsEnabled = false;
            DownloadButton.IsEnabled = false;

            var statusProgress = new Progress<string>(msg => LoadingText.Text = msg);
            _allEntries = await HydraSourceClient.FetchAllSourcesAsync(sources, statusProgress);

            var cv = CollectionViewSource.GetDefaultView(_allEntries);
            cv.Filter = FilterEntry;
            _currentSortProperty = "UploadDateParsed";
            _currentSortDir = ListSortDirection.Descending;
            cv.SortDescriptions.Add(new SortDescription("UploadDateParsed", ListSortDirection.Descending));
            _view = cv;
            ResultsView.ItemsSource = cv;

            LoadingPanel.Visibility = Visibility.Collapsed;
            RefreshButton.IsEnabled = true;
            UpdateResultCount();
        }

        private bool FilterEntry(object obj)
        {
            if (string.IsNullOrWhiteSpace(_searchText)) return true;
            return obj is HydraDownloadEntry e &&
                   e.Title.Contains(_searchText, StringComparison.OrdinalIgnoreCase);
        }

        private void ApplyFilter()
        {
            _view?.Refresh();
            UpdateResultCount();
        }

        private void UpdateResultCount()
        {
            if (_view == null) return;
            int shown = 0;
            foreach (var _ in _view) shown++;
            ResultCountText.Text = $"{shown:N0} of {_allEntries.Count:N0} games";
            ResultCountText.Visibility = Visibility.Visible;
        }

        private void SearchBox_TextChanged(object sender, TextChangedEventArgs e)
        {
            _searchText = SearchBox.Text;
            _searchTimer.Stop();
            _searchTimer.Start();
        }

        private void ResultsView_SelectionChanged(object sender, SelectionChangedEventArgs e)
        {
            bool hasSelection = ResultsView.SelectedItem is HydraDownloadEntry;
            DownloadButton.IsEnabled = hasSelection;

            if (ResultsView.SelectedItem is HydraDownloadEntry entry)
                SelectionInfoText.Text = $"{entry.FileSize}  ·  {entry.UploadDateFormatted}  ·  {entry.SourceName}";
            else
                SelectionInfoText.Text = "";
        }

        private void ResultsView_MouseDoubleClick(object sender, MouseButtonEventArgs e)
        {
            if (ResultsView.SelectedItem is HydraDownloadEntry)
                CommitSelection();
        }

        private async void Refresh_Click(object sender, RoutedEventArgs e)
        {
            ResultsView.ItemsSource = null;
            _view = null;
            _allEntries.Clear();
            SelectionInfoText.Text = "";
            DownloadButton.IsEnabled = false;
            await LoadSourcesAsync();
        }

        private void Download_Click(object sender, RoutedEventArgs e) => CommitSelection();

        private void Cancel_Click(object sender, RoutedEventArgs e)
        {
            DialogResult = false;
            Close();
        }

        private void CommitSelection()
        {
            if (ResultsView.SelectedItem is not HydraDownloadEntry entry) return;
            if (entry.Uris.Count == 0)
            {
                MessageBox.Show("This entry has no download URIs.", "No URIs",
                    MessageBoxButton.OK, MessageBoxImage.Warning);
                return;
            }
            SelectedDownload = entry;
            DialogResult = true;
            Close();
        }

        private void ColumnHeader_Click(object sender, RoutedEventArgs e)
        {
            if (e.OriginalSource is not GridViewColumnHeader header || header.Role == GridViewColumnHeaderRole.Padding)
                return;

            string cleanText = GetCleanHeaderText(header);
            if (!ColumnToProperty.TryGetValue(cleanText, out string? property))
                return;

            if (property == _currentSortProperty)
                _currentSortDir = _currentSortDir == ListSortDirection.Ascending
                    ? ListSortDirection.Descending
                    : ListSortDirection.Ascending;
            else
            {
                _currentSortDir = ListSortDirection.Ascending;
                _currentSortProperty = property;
            }

            if (_lastSortedHeader != null)
                _lastSortedHeader.Content = GetCleanHeaderText(_lastSortedHeader);
            header.Content = cleanText + (_currentSortDir == ListSortDirection.Ascending ? " ▲" : " ▼");
            _lastSortedHeader = header;

            ApplySort();
        }

        private static string GetCleanHeaderText(GridViewColumnHeader header)
            => (header.Content?.ToString() ?? "").Replace(" ▲", "").Replace(" ▼", "").Trim();

        private void ApplySort()
        {
            if (_view == null || _currentSortProperty == null) return;
            _view.SortDescriptions.Clear();
            _view.SortDescriptions.Add(new SortDescription(_currentSortProperty, _currentSortDir));
        }
    }
}
