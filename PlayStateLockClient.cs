using System;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading;
using System.Threading.Tasks;

namespace LudusaviWrap
{
    public class LockStatusResponse
    {
        [JsonPropertyName("locked")]
        public bool Locked { get; set; }

        [JsonPropertyName("device_id")]
        public string? DeviceId { get; set; }

        [JsonPropertyName("device_name")]
        public string? DeviceName { get; set; }

        [JsonPropertyName("stale")]
        public bool Stale { get; set; }
    }

    public class AcquireRequest
    {
        [JsonPropertyName("device_id")]
        public string DeviceId { get; set; } = "";

        [JsonPropertyName("device_name")]
        public string DeviceName { get; set; } = "";
    }

    public class AcquireConflictResponse
    {
        [JsonPropertyName("device_name")]
        public string? DeviceName { get; set; }
    }

    [JsonSourceGenerationOptions(WriteIndented = false)]
    [JsonSerializable(typeof(LockStatusResponse))]
    [JsonSerializable(typeof(AcquireRequest))]
    [JsonSerializable(typeof(AcquireConflictResponse))]
    internal partial class LockSourceGenerationContext : JsonSerializerContext
    {
    }

    public enum AcquireOutcome { Acquired, Conflict, ServerError, Unavailable }

    public class AcquireResult
    {
        public AcquireOutcome Outcome { get; init; }
        public string? ConflictDeviceName { get; init; }

        public static readonly AcquireResult Acquired  = new() { Outcome = AcquireOutcome.Acquired };
        public static readonly AcquireResult ServerError  = new() { Outcome = AcquireOutcome.ServerError };
        public static readonly AcquireResult Unavailable  = new() { Outcome = AcquireOutcome.Unavailable };

        public static AcquireResult ConflictWith(string? deviceName) =>
            new() { Outcome = AcquireOutcome.Conflict, ConflictDeviceName = deviceName };
    }

    public class PlayStateLockClient
    {
        private static readonly HttpClient HttpClient = new() { Timeout = TimeSpan.FromSeconds(10) };

        private readonly string _baseUrl;
        private readonly string _apiKey;
        private readonly string _deviceId;
        private readonly string _deviceName;

        public PlayStateLockClient(string serverUrl, string apiKey, string deviceId, string deviceName)
        {
            _baseUrl = serverUrl.TrimEnd('/');
            _apiKey = apiKey;
            _deviceId = deviceId;
            _deviceName = deviceName;
        }

        private HttpRequestMessage BuildRequest(HttpMethod method, string path)
        {
            var req = new HttpRequestMessage(method, _baseUrl + path);
            req.Headers.Authorization = new AuthenticationHeaderValue("Bearer", _apiKey);
            req.Headers.Add("X-Device-Id", _deviceId);
            return req;
        }

        public async Task<LockStatusResponse?> CheckLockAsync(string gameName)
        {
            try
            {
                using var req = BuildRequest(HttpMethod.Get, $"/locks/{Uri.EscapeDataString(gameName)}");
                using var resp = await HttpClient.SendAsync(req);
                if (!resp.IsSuccessStatusCode) return null;

                string json = await resp.Content.ReadAsStringAsync();
                return JsonSerializer.Deserialize(json, LockSourceGenerationContext.Default.LockStatusResponse);
            }
            catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException or JsonException)
            {
                return null;
            }
        }

        public async Task<AcquireResult> AcquireLockAsync(string gameName)
        {
            try
            {
                using var req = BuildRequest(HttpMethod.Post, $"/locks/{Uri.EscapeDataString(gameName)}/acquire");
                var body = new AcquireRequest { DeviceId = _deviceId, DeviceName = _deviceName };
                req.Content = JsonContent.Create(body, LockSourceGenerationContext.Default.AcquireRequest);

                using var resp = await HttpClient.SendAsync(req);

                if (resp.IsSuccessStatusCode)
                    return AcquireResult.Acquired;

                if ((int)resp.StatusCode == 409)
                {
                    string json = await resp.Content.ReadAsStringAsync();
                    var conflict = JsonSerializer.Deserialize(json, LockSourceGenerationContext.Default.AcquireConflictResponse);
                    return AcquireResult.ConflictWith(conflict?.DeviceName);
                }

                return AcquireResult.ServerError;
            }
            catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException or JsonException)
            {
                return AcquireResult.Unavailable;
            }
        }

        public async Task ReleaseLockAsync(string gameName)
        {
            try
            {
                using var req = BuildRequest(HttpMethod.Post, $"/locks/{Uri.EscapeDataString(gameName)}/release");
                await HttpClient.SendAsync(req);
            }
            catch
            {
                // Fire-and-forget; swallow all errors
            }
        }

        public async Task StartHeartbeatLoopAsync(string gameName, CancellationToken ct)
        {
            try
            {
                while (!ct.IsCancellationRequested)
                {
                    await Task.Delay(TimeSpan.FromSeconds(60), ct);

                    using var req = BuildRequest(HttpMethod.Post, $"/locks/{Uri.EscapeDataString(gameName)}/heartbeat");
                    try
                    {
                        await HttpClient.SendAsync(req, ct);
                    }
                    catch
                    {
                        // Ignore individual heartbeat failures; keep looping
                    }
                }
            }
            catch (OperationCanceledException)
            {
                // Normal shutdown
            }
        }
    }
}
