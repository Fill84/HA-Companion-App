using System.Text.Json.Serialization;

namespace HwmonHelper;

public sealed record HelloResponse(
    [property: JsonPropertyName("version")] string Version,
    [property: JsonPropertyName("capabilities")] string[] Capabilities);

public sealed record PollResponse(
    [property: JsonPropertyName("cpu_package_c")] double? CpuPackageC,
    [property: JsonPropertyName("cpu_core_avg_c")] double? CpuCoreAvgC);

public sealed record ErrorResponse(
    [property: JsonPropertyName("error")] string Error);

// AOT-safe source generator context. We don't enable AOT in Phase 2A
// but using a typed context now keeps the option open.
[JsonSerializable(typeof(HelloResponse))]
[JsonSerializable(typeof(PollResponse))]
[JsonSerializable(typeof(ErrorResponse))]
[JsonSourceGenerationOptions(WriteIndented = false)]
public partial class ProtocolJsonContext : JsonSerializerContext;
