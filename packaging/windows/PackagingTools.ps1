<#
  Shared helpers for build-installer.ps1 and verify-package.ps1 (dot-sourced).
  Nothing here needs Visual Studio's dumpbin: the PE import tables and the
  resources are read directly, so verification works on any Windows machine.
#>

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

public static class PeTools
{
    // ---- Imports: the DLL names a PE file (64-bit) links, normal and delay-loaded.
    public static List<string> Imports(string path)
    {
        var names = new List<string>();
        byte[] b = File.ReadAllBytes(path);
        if (b.Length < 0x40 || b[0] != 'M' || b[1] != 'Z') return names;
        int pe = BitConverter.ToInt32(b, 0x3C);
        if (BitConverter.ToUInt32(b, pe) != 0x00004550) return names;           // "PE\0\0"
        int sections = BitConverter.ToUInt16(b, pe + 6);
        int optSize = BitConverter.ToUInt16(b, pe + 20);
        int opt = pe + 24;
        bool pe64 = BitConverter.ToUInt16(b, opt) == 0x20B;
        int dirs = opt + (pe64 ? 112 : 96);
        int sec = opt + optSize;
        Func<uint, int> rva2off = rva =>
        {
            for (int i = 0; i < sections; i++)
            {
                int s = sec + i * 40;
                uint va = BitConverter.ToUInt32(b, s + 12), raw = BitConverter.ToUInt32(b, s + 20);
                uint vs = BitConverter.ToUInt32(b, s + 8), rs = BitConverter.ToUInt32(b, s + 16);
                if (rva >= va && rva < va + Math.Max(vs, rs)) return (int)(rva - va + raw);
            }
            return -1;
        };
        Func<int, string> cstr = off =>
        {
            int e = off; while (e < b.Length && b[e] != 0) e++;
            return Encoding.ASCII.GetString(b, off, e - off);
        };
        // Import directory (index 1): 20-byte descriptors, Name at +12.
        uint impRva = BitConverter.ToUInt32(b, dirs + 8 * 1);
        if (impRva != 0)
        {
            int o = rva2off(impRva);
            while (o > 0 && BitConverter.ToUInt32(b, o + 12) != 0)
            {
                int n = rva2off(BitConverter.ToUInt32(b, o + 12));
                if (n > 0) names.Add(cstr(n));
                o += 20;
            }
        }
        // Delay-load directory (index 13): 32-byte descriptors, Name RVA at +4.
        uint delRva = BitConverter.ToUInt32(b, dirs + 8 * 13);
        if (delRva != 0)
        {
            int o = rva2off(delRva);
            while (o > 0 && BitConverter.ToUInt32(b, o + 4) != 0)
            {
                int n = rva2off(BitConverter.ToUInt32(b, o + 4));
                if (n > 0) names.Add(cstr(n));
                o += 32;
            }
        }
        return names;
    }

    // ---- Subsystem of a PE file: 2 = Windows GUI, 3 = console.
    public static int Subsystem(string path)
    {
        byte[] b = File.ReadAllBytes(path);
        int pe = BitConverter.ToInt32(b, 0x3C);
        return BitConverter.ToUInt16(b, pe + 24 + 68);
    }

    // ---- Resources: how many resources of a type the file holds.
    delegate bool EnumProc(IntPtr h, IntPtr type, IntPtr name, IntPtr param);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern IntPtr LoadLibraryEx(string file, IntPtr h, uint flags);
    [DllImport("kernel32.dll")] static extern bool FreeLibrary(IntPtr h);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool EnumResourceNames(IntPtr h, IntPtr type, EnumProc proc, IntPtr param);

    public static int ResourceCount(string path, int type)
    {
        IntPtr h = LoadLibraryEx(path, IntPtr.Zero, 0x2 /* LOAD_LIBRARY_AS_DATAFILE */);
        if (h == IntPtr.Zero) return -1;
        int count = 0;
        try { EnumResourceNames(h, (IntPtr)type, (a, t, n, p) => { count++; return true; }, IntPtr.Zero); }
        finally { FreeLibrary(h); }
        return count;
    }

    // ---- Whether a file contains a byte string, as ASCII and as UTF-16.
    public static bool Contains(string path, string text)
    {
        byte[] b = File.ReadAllBytes(path);
        return IndexOf(b, Encoding.ASCII.GetBytes(text)) >= 0
            || IndexOf(b, Encoding.Unicode.GetBytes(text)) >= 0;
    }
    static int IndexOf(byte[] hay, byte[] needle)
    {
        if (needle.Length == 0) return 0;
        for (int i = 0; i <= hay.Length - needle.Length; i++)
        {
            if (hay[i] != needle[0]) continue;
            int j = 1; while (j < needle.Length && hay[i + j] == needle[j]) j++;
            if (j == needle.Length) return i;
        }
        return -1;
    }
}
'@ -ErrorAction Stop

$script:RT_ICON_GROUP = 14
$script:RT_VERSION = 16
$script:RT_MANIFEST = 24

function Get-RepoRoot {
    (Resolve-Path (Join-Path (Split-Path -Parent $PSCommandPath) '..\..')).Path
}

# The version from Cargo.toml's [package] table: the one source of truth.
function Get-CargoVersion([string]$Repo) {
    $inPackage = $false
    foreach ($line in Get-Content (Join-Path $Repo 'Cargo.toml')) {
        if ($line -match '^\s*\[(.+)\]\s*$') { $inPackage = ($Matches[1] -eq 'package'); continue }
        if ($inPackage -and $line -match '^\s*version\s*=\s*"([^"]+)"') { return $Matches[1] }
    }
    throw 'Could not read [package] version from Cargo.toml'
}

# Windows-provided DLLs: API-set stubs, or a file in the system directory.
function Test-SystemDll([string]$Name) {
    if ($Name -match '^(api|ext)-ms-') { return $true }
    return Test-Path (Join-Path $env:SystemRoot "System32\$Name")
}

# Every PE file of a staged tree.
function Get-PeFiles([string]$Root) {
    Get-ChildItem $Root -Recurse -File | Where-Object { $_.Extension -in '.exe', '.dll' }
}

# The dependency audit of a tree: for every PE file, each imported DLL must be
# a DLL in the application directory (the root, where omatree.exe is: that is
# where Windows looks for the DLLs of the exe and of the plugins it loads) or a
# Windows system DLL. Returns the list of problems (empty when clean).
function Test-Dependencies([string]$Root) {
    $problems = @()
    $inTree = @{}
    foreach ($f in Get-ChildItem $Root -File | Where-Object { $_.Extension -in '.exe', '.dll' }) { $inTree[$f.Name.ToLowerInvariant()] = $f.FullName }
    foreach ($f in Get-PeFiles $Root) {
        foreach ($dll in [PeTools]::Imports($f.FullName)) {
            $key = $dll.ToLowerInvariant()
            if ($inTree.ContainsKey($key)) { continue }
            if (Test-SystemDll $dll) { continue }
            $problems += "$($f.FullName.Substring($Root.Length + 1)) imports $dll, which is neither in the application directory nor a Windows system DLL"
        }
    }
    return $problems
}
