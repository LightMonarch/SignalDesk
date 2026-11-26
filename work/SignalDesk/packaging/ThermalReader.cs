using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.IO.MemoryMappedFiles;
using System.Security.AccessControl;
using System.Security.Principal;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Web.Script.Serialization;
using LibreHardwareMonitor.Hardware;
using Microsoft.Win32;

internal static class ThermalReader
{
    private const int MappingBytes = 65536;
    private const string Source = "LibreHardwareMonitor sensor helper";

    private static bool Elevated()
    {
        using (WindowsIdentity identity = WindowsIdentity.GetCurrent())
            return new WindowsPrincipal(identity).IsInRole(WindowsBuiltInRole.Administrator);
    }

    private static bool DriverRegistered()
    {
        using (RegistryKey driver = Registry.LocalMachine.OpenSubKey(@"SYSTEM\CurrentControlSet\Services\PawnIO", false))
            return driver != null;
    }

    private static void Update(IHardware hardware, List<object> rows)
    {
        hardware.Update();
        foreach (ISensor sensor in hardware.Sensors)
        {
            if (!sensor.Value.HasValue || float.IsNaN(sensor.Value.Value) || float.IsInfinity(sensor.Value.Value))
                continue;
            if (sensor.SensorType != SensorType.Temperature && sensor.SensorType != SensorType.Power)
                continue;
            rows.Add(new
            {
                Name = sensor.Name,
                Identifier = sensor.Identifier.ToString(),
                Parent = hardware.Identifier.ToString(),
                SensorType = sensor.SensorType.ToString(),
                Value = sensor.Value.Value,
                Min = Finite(sensor.Min),
                Max = Finite(sensor.Max),
                Source
            });
        }
        foreach (IHardware child in hardware.SubHardware)
            Update(child, rows);
    }

    private static float? Finite(float? value)
    {
        return value.HasValue && (float.IsNaN(value.Value) || float.IsInfinity(value.Value)) ? null : value;
    }

    private static void Publish(MemoryMappedViewAccessor view, JavaScriptSerializer json, ref int sequence,
        string state, string detail, List<object> sensors)
    {
        byte[] bytes = Encoding.UTF8.GetBytes(json.Serialize(new
        {
            state, detail, sensors,
            timestamp = (long)(DateTime.UtcNow - new DateTime(1970, 1, 1)).TotalSeconds
        }));
        if (bytes.Length > MappingBytes - 8) return;

        // An odd sequence marks an in-progress write.
        view.Write(0, unchecked(++sequence));
        Thread.MemoryBarrier();
        view.Write(4, bytes.Length);
        view.WriteArray(8, bytes, 0, bytes.Length);
        Thread.MemoryBarrier();
        view.Write(0, unchecked(++sequence));
    }

    private static int Main(string[] args)
    {
        if (args.Length == 1 && args[0] == "--check")
        {
            // Build smoke check. It resolves the library without opening hardware or installing a driver.
            try
            {
                return typeof(Computer).Assembly.GetName().Version != null ? 0 : 1;
            }
            catch { return 1; }
        }

        int parentPid;
        long parentStart;
        if (args.Length != 3 || !Regex.IsMatch(args[0], "^[a-f0-9]{32}$")
            || !int.TryParse(args[1], out parentPid) || !long.TryParse(args[2], out parentStart))
            return 2;

        try
        {
            using (Process parent = Process.GetProcessById(parentPid))
            using (WindowsIdentity identity = WindowsIdentity.GetCurrent())
            {
                if (parent.StartTime.ToUniversalTime().ToFileTimeUtc() != parentStart)
                    return 2;

                var security = new MemoryMappedFileSecurity();
                security.AddAccessRule(new AccessRule<MemoryMappedFileRights>(identity.User,
                    MemoryMappedFileRights.FullControl, AccessControlType.Allow));
                string name = @"Local\SiliconDesk.Thermals." + args[0];
                using (MemoryMappedFile mapping = MemoryMappedFile.CreateNew(name, MappingBytes,
                    MemoryMappedFileAccess.ReadWrite, MemoryMappedFileOptions.None, security,
                    HandleInheritability.None))
                using (MemoryMappedViewAccessor view = mapping.CreateViewAccessor())
                using (EventWaitHandle pause = EventWaitHandle.OpenExisting(name + ".Pause", EventWaitHandleRights.Synchronize))
                {
                    var json = new JavaScriptSerializer();
                    int sequence = 0;
                    if (!Elevated())
                    {
                        Publish(view, json, ref sequence, "needsAdmin", "The sensor helper requires Administrator access.", new List<object>());
                        parent.WaitForExit(5000);
                        return 3;
                    }
                    if (!DriverRegistered())
                    {
                        while (!parent.HasExited)
                        {
                            Publish(view, json, ref sequence, "driverMissing",
                                "PawnIO is not installed. Install the official driver and restart Silicon Desk.", new List<object>());
                            parent.WaitForExit(3000);
                        }
                        return 0;
                    }

                    // Keep one hardware instance for the session. Graphics are queried in the Rust host
                    // using the Windows power state and the appropriate GPU provider.
                    var computer = new Computer { IsCpuEnabled = true, IsGpuEnabled = false };
                    try
                    {
                        Publish(view, json, ref sequence, "starting", "Opening CPU sensors through LibreHardwareMonitor.", new List<object>());
                        computer.Open();
                        while (!parent.HasExited)
                        {
                            if (!pause.WaitOne(0))
                            {
                                var rows = new List<object>();
                                foreach (IHardware hardware in computer.Hardware)
                                    Update(hardware, rows);
                                bool hasReadings = rows.Count > 0;
                                Publish(view, json, ref sequence, hasReadings ? "active" : "noSensors",
                                    hasReadings ? "The sensor helper is collecting CPU hardware readings."
                                    : "PawnIO is installed, but the CPU did not return readable sensors. A driver policy or firmware limitation may apply.", rows);
                            }
                            parent.WaitForExit(1500);
                        }
                    }
                    catch (Exception ex)
                    {
                        string status = ex is UnauthorizedAccessException ? "driverDenied" : "failed";
                        string detail = ex is UnauthorizedAccessException
                            ? "Windows or a security policy denied access to the sensor driver."
                            : "The hardware provider failed (" + ex.GetType().Name + "). Check PawnIO and Windows driver policies.";
                        while (!parent.HasExited)
                        {
                            Publish(view, json, ref sequence, status, detail, new List<object>());
                            parent.WaitForExit(3000);
                        }
                    }
                    finally { computer.Close(); }
                }
            }
            return 0;
        }
        catch { return 4; }
    }
}
