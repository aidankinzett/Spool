using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Windows;
using System.Windows.Data;
using System.Windows.Media;
using System.Windows.Media.Imaging;

namespace LudusaviWrap
{
    public class StringToImageConverter : IValueConverter
    {
        public object? Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            if (value is not string path || string.IsNullOrEmpty(path)) return null;
            try
            {
                var bi = new BitmapImage();
                bi.BeginInit();
                bi.UriSource = new Uri(path);
                bool isHttp = path.StartsWith("http://", StringComparison.OrdinalIgnoreCase)
                           || path.StartsWith("https://", StringComparison.OrdinalIgnoreCase);
                if (isHttp)
                {
                    bi.CacheOption = BitmapCacheOption.Default;
                }
                else
                {
                    if (!File.Exists(path)) return null;
                    bi.CacheOption = BitmapCacheOption.OnLoad;
                }
                bi.EndInit();
                if (!isHttp) bi.Freeze();
                return bi;
            }
            catch (Exception ex)
            {
                App.Log($"StringToImageConverter exception for '{path}': {ex.Message}");
                return null;
            }
        }
        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class CoverToBackgroundBrushConverter : IValueConverter
    {
        private static readonly Brush DefaultBrush;

        static CoverToBackgroundBrushConverter()
        {
            var brush = new LinearGradientBrush(
                Color.FromRgb(0x1F, 0x2E, 0x3D),
                Color.FromRgb(0x0A, 0x0F, 0x14),
                new Point(0, 0),
                new Point(1, 1)
            );
            brush.Freeze();
            DefaultBrush = brush;
        }

        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            if (value is not string path || string.IsNullOrEmpty(path) || !File.Exists(path))
                return DefaultBrush;

            try
            {
                using (var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read))
                {
                    var decoder = BitmapDecoder.Create(stream, BitmapCreateOptions.None, BitmapCacheOption.OnLoad);
                    var frame = decoder.Frames[0];
                    
                    // Scale to 2x2
                    var scaled = new TransformedBitmap(frame, new ScaleTransform(2.0 / frame.PixelWidth, 2.0 / frame.PixelHeight));
                    var bmp = new WriteableBitmap(scaled);
                    
                    byte[] pixels = new byte[16];
                    bmp.CopyPixels(pixels, 8, 0);

                    // BGRA format
                    var color1 = Color.FromRgb(pixels[2], pixels[1], pixels[0]);
                    var color2 = Color.FromRgb(pixels[14], pixels[13], pixels[12]);

                    // Darken to make suitable for background
                    var c1 = Color.FromRgb((byte)(color1.R * 0.4), (byte)(color1.G * 0.4), (byte)(color1.B * 0.4));
                    var c2 = Color.FromRgb((byte)(color2.R * 0.25), (byte)(color2.G * 0.25), (byte)(color2.B * 0.25));

                    var brush = new LinearGradientBrush(c1, c2, new Point(0, 0), new Point(1, 1));
                    brush.Freeze();
                    return brush;
                }
            }
            catch (Exception ex)
            {
                App.Log($"CoverToBackgroundBrushConverter exception for '{path}': {ex.Message}");
                return DefaultBrush;
            }
        }

        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class BooleanToVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
            => value is bool b && b ? Visibility.Visible : Visibility.Collapsed;
        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class InverseBooleanToVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
            => value is bool b && b ? Visibility.Collapsed : Visibility.Visible;
        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class NullToVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
            => value != null ? Visibility.Visible : Visibility.Collapsed;
        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class InverseNullToVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
            => value == null ? Visibility.Visible : Visibility.Collapsed;
        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class EmptyStringToVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
            => value is string s && string.IsNullOrEmpty(s) ? Visibility.Visible : Visibility.Collapsed;
        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class InverseEmptyStringToVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
            => value is string s && !string.IsNullOrEmpty(s) ? Visibility.Visible : Visibility.Collapsed;
        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class RelativeDateConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            if (value is not DateTime dt) return "Never";
            var local = dt.Kind == DateTimeKind.Utc ? dt.ToLocalTime() : dt;
            var elapsed = DateTime.Now - local;
            if (elapsed.TotalMinutes < 2) return "Just now";
            if (elapsed.TotalHours < 1) return $"{(int)elapsed.TotalMinutes} min ago";
            if (elapsed.TotalHours < 24) return $"{(int)elapsed.TotalHours}h ago";
            if (elapsed.TotalDays < 2) return "Yesterday";
            if (elapsed.TotalDays < 7) return $"{(int)elapsed.TotalDays} days ago";
            if (elapsed.TotalDays < 365) return local.ToString("MMM d");
            return local.ToString("MMM d, yyyy");
        }
        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class AbsoluteDateConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            if (value is DateTime dt)
                return dt.ToString("MMMM d, yyyy");
            return "—";
        }
        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class PlaytimeConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            if (value is not int minutes || minutes <= 0) return "—";
            int h = minutes / 60, m = minutes % 60;
            if (h == 0) return $"{m} min";
            return m == 0 ? $"{h} h" : $"{h} h {m} min";
        }
        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class PlaytimeCompactConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            if (value is not int minutes || minutes <= 0) return "";
            int h = minutes / 60, m = minutes % 60;
            if (h == 0) return $"{m}m";
            return m == 0 ? $"{h}h" : $"{h}h {m}m";
        }
        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class FileSizeMbConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            if (value is not double mb || mb <= 0) return "—";
            if (mb < 1024) return $"{mb:0.0} MB";
            if (mb < 1024 * 1024) return $"{mb / 1024:0.0} GB";
            return $"{mb / (1024 * 1024):0.0} TB";
        }
        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class GenreListConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            if (value is List<string> genres && genres.Count > 0)
                return string.Join(" · ", genres);
            return "—";
        }
        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class CoverInitialsConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            if (value is not string name || string.IsNullOrEmpty(name)) return "";
            var words = name.Split(' ', StringSplitOptions.RemoveEmptyEntries);
            if (words.Length == 1)
                return name.Length >= 2 ? name[..2].ToUpper() : name.ToUpper();
            return $"{char.ToUpper(words[0][0])}{char.ToUpper(words[1][0])}";
        }
        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class YearConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
            => value is DateTime dt ? dt.Year.ToString() : "—";
        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }

    public class PositiveIntToVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
            => value is int n && n > 0 ? Visibility.Visible : Visibility.Collapsed;
        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
            => throw new NotImplementedException();
    }
}
