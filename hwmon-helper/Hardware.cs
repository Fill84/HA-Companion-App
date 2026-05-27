using LibreHardwareMonitor.Hardware;

namespace HwmonHelper;

/// <summary>
/// Single-purpose wrapper around LHM's Computer that owns the lifecycle and
/// exposes a Snapshot() returning Phase 2A's MVP fields. Per-core + voltages
/// + GPU + SMART will extend Snapshot() in Phase 2B.
/// </summary>
public sealed class HardwareReader : IDisposable
{
    private readonly Computer _computer;
    private bool _disposed;

    public HardwareReader()
    {
        _computer = new Computer
        {
            IsCpuEnabled = true,
            // Phase 2B: enable Mainboard, Gpu, Storage, Memory, Battery, Network.
        };
        _computer.Open();
    }

    /// <summary>
    /// Log a one-line description per CPU sensor to stderr so the dev / log
    /// reader can confirm what LHM detected on this hardware.
    /// </summary>
    public void DescribeToStderr(TextWriter stderr)
    {
        foreach (var hw in _computer.Hardware)
        {
            stderr.WriteLine($"[hwmon] hardware: {hw.HardwareType} {hw.Name} ({hw.Identifier})");
            hw.Update();
            foreach (var s in hw.Sensors)
            {
                stderr.WriteLine($"[hwmon]   sensor: {s.SensorType} '{s.Name}' = {s.Value?.ToString("F1") ?? "null"} ({s.Identifier})");
            }
        }
    }

    /// <summary>
    /// Take a fresh snapshot. Returns the package CPU temperature when one is
    /// reported by LHM, and the average of CPU core temperatures as a backup.
    /// Either may be null on hardware that doesn't expose them.
    /// </summary>
    public PollResponse Snapshot()
    {
        double? package = null;
        var cores = new List<double>();

        foreach (var hw in _computer.Hardware)
        {
            if (hw.HardwareType is not (HardwareType.Cpu)) continue;
            hw.Update();

            foreach (var s in hw.Sensors)
            {
                if (s.SensorType != SensorType.Temperature) continue;
                if (!s.Value.HasValue) continue;

                var v = (double)s.Value.Value;
                if (!(v > 0 && v < 150)) continue; // sanity range

                var name = s.Name?.ToLowerInvariant() ?? string.Empty;

                if (package is null && (
                    name.Contains("package") ||
                    name.Contains("tdie") ||
                    name.Contains("ccd average") ||
                    name.Contains("cpu total")))
                {
                    package = v;
                }
                // Match individual cores only — exclude LHM-computed aggregates
                // ("Core Average", "Core Max") and the non-temperature
                // "Distance to TjMax" / "TjMax" sensors that also live under
                // SensorType.Temperature on Intel CPUs.
                else if (name.Contains("cpu core #")
                         && !name.Contains("distance")
                         && !name.Contains("tjmax"))
                {
                    cores.Add(v);
                }
            }
        }

        double? coreAvg = cores.Count > 0 ? cores.Average() : null;
        return new PollResponse(package, coreAvg);
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        _computer.Close();
    }
}
