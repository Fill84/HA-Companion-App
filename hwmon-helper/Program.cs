using System.Text.Json;
using LibreHardwareMonitor.Hardware;
using LibreHardwareMonitor.PawnIo;

// No elevation, service management, downloads, network or credential input.
// The optional, separately installed official PawnIO driver must already work.
if (args.Length != 1 || args[0] != "--serve") return 2;
Computer? computer = null;
try
{
    string? line;
    while ((line = Console.ReadLine()) is not null)
    {
        if (line.Length > 128) return 2;
        var request = JsonSerializer.Deserialize<Request>(line);
        if (request is null || request.protocol != 1) return 2;
        var status = "ok";
        var readings = new List<Reading>();
        try
        {
            if (!PawnIo.IsInstalled) status = "driver_missing";
            else if (PawnIo.Version < new Version(2, 2)) status = "driver_unsupported";
            else
            {
                if (computer is null)
                {
                    computer = new Computer { IsCpuEnabled = true };
                    computer.Open();
                }
                foreach (var hardware in computer.Hardware)
                {
                    if (hardware.HardwareType != HardwareType.Cpu) continue;
                    hardware.Update();
                    foreach (var sensor in hardware.Sensors)
                    {
                        if (sensor.SensorType == SensorType.Temperature
                            && sensor.Value is float value && float.IsFinite(value))
                        {
                            readings.Add(new Reading(hardware.Identifier.ToString(), sensor.Name, value));
                        }
                    }
                }
                if (readings.Count == 0) status = "unsupported";
            }
        }
        catch
        {
            status = "provider_error";
            readings.Clear();
            computer?.Close();
            computer = null;
        }
        Console.WriteLine(JsonSerializer.Serialize(new
        {
            protocol = 1, request_id = request.request_id,
            provider = "librehardwaremonitor", provider_version = "0.9.6",
            status, readings
        }));
    }
}
finally
{
    computer?.Close();
}
return 0;

record Request(int protocol, ulong request_id);
record Reading(string hardware_id, string label, float celsius);
