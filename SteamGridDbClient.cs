using System;
using System.Collections.Generic;
using System.IO;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading.Tasks;

namespace LudusaviWrap
{
    public class SgdbSearchResult
    {
        [JsonPropertyName("id")]
        public int Id { get; set; }

        [JsonPropertyName("name")]
        public string Name { get; set; } = "";
    }

    public class SgdbSearchResponse
    {
        [JsonPropertyName("data")]
        public List<SgdbSearchResult>? Data { get; set; }
    }

    public class SgdbGridResult
    {
        [JsonPropertyName("url")]
        public string Url { get; set; } = "";

        [JsonPropertyName("mime")]
        public string Mime { get; set; } = "";
    }

    public class SgdbGridResponse
    {
        [JsonPropertyName("data")]
        public List<SgdbGridResult>? Data { get; set; }
    }

    [JsonSourceGenerationOptions(WriteIndented = true)]
    [JsonSerializable(typeof(SgdbSearchResponse))]
    [JsonSerializable(typeof(SgdbGridResponse))]
    internal partial class SgdbSourceGenerationContext : JsonSerializerContext
    {
    }

    public class SteamGridDbClient
    {
        private static readonly HttpClient HttpClient = new HttpClient { Timeout = TimeSpan.FromSeconds(30) };
        private readonly string _apiKey;

        public SteamGridDbClient(string apiKey)
        {
            _apiKey = apiKey;
        }

        private HttpRequestMessage CreateRequest(string url)
        {
            var req = new HttpRequestMessage(HttpMethod.Get, url);
            req.Headers.Authorization = new AuthenticationHeaderValue("Bearer", _apiKey);
            return req;
        }

        public async Task<List<SgdbSearchResult>> SearchGameAsync(string query)
        {
            string url = $"https://www.steamgriddb.com/api/v2/search/autocomplete/{Uri.EscapeDataString(query)}";
            using var req = CreateRequest(url);
            using var resp = await HttpClient.SendAsync(req);
            resp.EnsureSuccessStatusCode();

            string json = await resp.Content.ReadAsStringAsync();
            var searchResponse = JsonSerializer.Deserialize(json, SgdbSourceGenerationContext.Default.SgdbSearchResponse);
            return searchResponse?.Data ?? new List<SgdbSearchResult>();
        }

        // Downloads the first result from an SGDB list endpoint to destPath (including extension).
        // Returns the saved path on success, null if no results or download fails.
        private async Task<string?> DownloadFirstAsync(string apiUrl, string destPathNoExt)
        {
            using var req = CreateRequest(apiUrl);
            using var resp = await HttpClient.SendAsync(req);
            if (!resp.IsSuccessStatusCode) return null;

            string json = await resp.Content.ReadAsStringAsync();
            var gridResponse = JsonSerializer.Deserialize(json, SgdbSourceGenerationContext.Default.SgdbGridResponse);
            if (gridResponse?.Data == null || gridResponse.Data.Count == 0) return null;

            string imageUrl = gridResponse.Data[0].Url;
            string mimeType = gridResponse.Data[0].Mime;
            string ext = mimeType switch
            {
                "image/png" => ".png",
                "image/webp" => ".webp",
                _ => ".jpg"
            };

            using var imgResp = await HttpClient.GetAsync(imageUrl);
            if (!imgResp.IsSuccessStatusCode) return null;
            byte[] imgBytes = await imgResp.Content.ReadAsByteArrayAsync();

            string destPath = destPathNoExt + ext;
            Directory.CreateDirectory(Path.GetDirectoryName(destPath)!);
            await File.WriteAllBytesAsync(destPath, imgBytes);
            return destPath;
        }

        // Horizontal grid (460x215 / 920x430) — used for the Armoury Crate cover and as fallback.
        public Task<string?> DownloadGridImageAsync(int gameId, string safeGameName, string destDir) =>
            DownloadFirstAsync(
                $"https://www.steamgriddb.com/api/v2/grids/game/{gameId}?dimensions=460x215,920x430",
                Path.Combine(destDir, safeGameName));

        // Portrait/vertical grid (600x900 etc.) — shown in the modern Steam library grid view.
        public Task<string?> DownloadPortraitAsync(int gameId, string destPathNoExt) =>
            DownloadFirstAsync(
                $"https://www.steamgriddb.com/api/v2/grids/game/{gameId}?dimensions=600x900,342x482,660x930",
                destPathNoExt);

        // Hero banner image — shown at the top of the game's Steam page.
        public Task<string?> DownloadHeroAsync(int gameId, string destPathNoExt) =>
            DownloadFirstAsync(
                $"https://www.steamgriddb.com/api/v2/heroes/game/{gameId}",
                destPathNoExt);

        // Logo (transparent PNG) — overlaid on the hero image.
        public Task<string?> DownloadLogoAsync(int gameId, string destPathNoExt) =>
            DownloadFirstAsync(
                $"https://www.steamgriddb.com/api/v2/logos/game/{gameId}",
                destPathNoExt);
    }
}
