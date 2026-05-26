using System;
using System.Collections.Generic;
using System.Net.Http;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading.Tasks;

namespace LudusaviWrap
{
    public sealed class HydraDownloadEntry
    {
        [JsonPropertyName("title")]
        public string Title { get; set; } = "";

        [JsonPropertyName("uris")]
        public List<string> Uris { get; set; } = new();

        [JsonPropertyName("uploadDate")]
        public string UploadDate { get; set; } = "";

        [JsonPropertyName("fileSize")]
        public string FileSize { get; set; } = "";

        [JsonIgnore]
        public string SourceName { get; set; } = "";

        [JsonIgnore]
        public string UploadDateFormatted =>
            DateTime.TryParse(UploadDate, out var dt)
                ? dt.ToString("yyyy-MM-dd")
                : UploadDate.Length >= 10 ? UploadDate[..10] : UploadDate;

        [JsonIgnore]
        public DateTime UploadDateParsed =>
            DateTime.TryParse(UploadDate, out var dt) ? dt : DateTime.MinValue;
    }

    public sealed class HydraSourceFile
    {
        [JsonPropertyName("name")]
        public string Name { get; set; } = "";

        [JsonPropertyName("downloads")]
        public List<HydraDownloadEntry>? Downloads { get; set; }
    }

    [JsonSourceGenerationOptions(WriteIndented = false)]
    [JsonSerializable(typeof(HydraSourceFile))]
    internal partial class HydraSourceGenerationContext : JsonSerializerContext { }

    public static class HydraSourceClient
    {
        private static readonly HttpClient HttpClient = new() { Timeout = TimeSpan.FromSeconds(60) };

        public static async Task<HydraSourceFile?> FetchSourceAsync(string url)
        {
            using var resp = await HttpClient.GetAsync(url);
            resp.EnsureSuccessStatusCode();
            string json = await resp.Content.ReadAsStringAsync();
            return JsonSerializer.Deserialize(json, HydraSourceGenerationContext.Default.HydraSourceFile);
        }

        public static async Task<List<HydraDownloadEntry>> FetchAllSourcesAsync(
            List<string> urls,
            IProgress<string>? statusProgress = null)
        {
            var results = new List<HydraDownloadEntry>();
            foreach (var url in urls)
            {
                try
                {
                    statusProgress?.Report($"Fetching {new Uri(url).Host}...");
                    var source = await FetchSourceAsync(url);
                    if (source?.Downloads == null) continue;
                    foreach (var d in source.Downloads)
                    {
                        d.SourceName = source.Name;
                        results.Add(d);
                    }
                }
                catch (Exception ex)
                {
                    App.Log($"Failed to fetch source {url}: {ex.Message}");
                }
            }
            return results;
        }
    }
}
