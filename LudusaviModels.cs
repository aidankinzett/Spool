using System.Collections.Generic;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace LudusaviWrap
{
    public class ProcessResult
    {
        public int    ExitCode { get; set; }
        public string Output   { get; set; } = "";
        public string Error    { get; set; } = "";
    }

    public class LudusaviApiOutput
    {
        [JsonPropertyName("errors")]
        public LudusaviApiErrors? Errors { get; set; }

        [JsonPropertyName("overall")]
        public LudusaviApiOverall? Overall { get; set; }
    }

    public class LudusaviApiErrors
    {
        [JsonPropertyName("unknownGames")]
        public List<string>? UnknownGames { get; set; }

        [JsonPropertyName("cloudConflict")]
        public JsonElement? CloudConflict { get; set; }

        [JsonPropertyName("cloudSyncFailed")]
        public JsonElement? CloudSyncFailed { get; set; }
    }

    public class LudusaviApiOverall
    {
        [JsonPropertyName("totalGames")]
        public int TotalGames { get; set; }
    }

    [JsonSourceGenerationOptions(WriteIndented = false)]
    [JsonSerializable(typeof(LudusaviApiOutput))]
    internal partial class LudusaviOutputContext : JsonSerializerContext { }
}
