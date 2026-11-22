param(
    [string]$DeploymentDir = (Join-Path (Split-Path -Parent $PSScriptRoot) 'build/qt-preview'),
    [string]$OutputExe = (Join-Path (Split-Path -Parent $PSScriptRoot) 'build/Instalar-OpenClient.exe')
)
$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot

if (-not (Test-Path -LiteralPath (Join-Path $DeploymentDir 'bin/openclient-qt.exe'))) {
    throw 'Primero compila la vista previa con ./scripts/build-qt.ps1.'
}
$qtCore = Get-Item -LiteralPath (Join-Path $DeploymentDir 'bin/Qt6Core.dll')
$qtVersion = [version]::new($qtCore.VersionInfo.ProductMajorPart, $qtCore.VersionInfo.ProductMinorPart, $qtCore.VersionInfo.ProductBuildPart)
if ($qtVersion -lt [version]'6.11.3') {
    throw "Qt $qtVersion no incluye todos los parches requeridos. Recompila con Qt 6.11.3 o posterior."
}

$DeploymentDir = (Resolve-Path -LiteralPath $DeploymentDir).Path
$OutputExe = [IO.Path]::GetFullPath($OutputExe)
$outputFolder = Split-Path -Parent $OutputExe
$staging = Join-Path $outputFolder ("installer-staging-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $staging | Out-Null

try {
Add-Type -AssemblyName System.IO.Compression.FileSystem
Add-Type -AssemblyName System.IO.Compression
$payloadZip = Join-Path $staging 'payload.zip'
$manifest = Join-Path $staging 'payload.list'
$vsInstaller = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
$vswhere = Join-Path $vsInstaller 'Microsoft Visual Studio/Installer/vswhere.exe'
if (-not (Test-Path -LiteralPath $vswhere)) { throw 'No se encontró Visual Studio para obtener el redistribuible x64.' }
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw 'No se encontró Visual Studio con soporte de C++.' }
$crt = Get-ChildItem -Path (Join-Path $vs 'VC/Redist/MSVC/*/x64/Microsoft.VC143.CRT') -Directory |
    Sort-Object { [version]$_.Parent.Parent.Name } -Descending | Select-Object -First 1
if (-not $crt) { throw 'No se encontró el redistribuible Microsoft.VC143.CRT para x64.' }

$files = [Collections.Generic.Dictionary[string, string]]::new([StringComparer]::OrdinalIgnoreCase)
foreach ($file in Get-ChildItem -LiteralPath $DeploymentDir -Recurse -File) {
    $relative = $file.FullName.Substring($DeploymentDir.Length + 1).Replace('\', '/')
    if ($relative.StartsWith('plugins/qmltooling/', [StringComparison]::OrdinalIgnoreCase)) { continue }
    if ($relative -in @('bin/openclient-qt.exe', 'bin/qt.conf', 'bin/modpack/official.mrpack', 'share/openclient/licenses/octicons-LICENSE.txt') -or
        $relative -match '^(?:bin|lib|plugins|qml)/.+\.dll$|^qml/(?:.*/)?qmldir$|^translations/qt[_-].+\.qm$') {
        if ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "No se puede empaquetar un enlace: $relative" }
        $files.Add($relative, $file.FullName)
    }
}
foreach ($file in Get-ChildItem -LiteralPath $crt.FullName -Filter '*.dll' -File) {
    $files["bin/$($file.Name)"] = $file.FullName
}
foreach ($required in @('bin/openclient-qt.exe', 'bin/openclient_ffi.dll', 'bin/qt.conf', 'bin/Qt6Core.dll',
    'bin/msvcp140.dll', 'bin/msvcp140_1.dll', 'bin/msvcp140_2.dll', 'bin/vcruntime140.dll', 'bin/vcruntime140_1.dll',
    'plugins/platforms/qwindows.dll', 'plugins/platforms/qoffscreen.dll')) {
    if (-not $files.ContainsKey($required)) { throw "Falta un archivo de distribución: $required" }
}
$entries = @($files.Keys | Sort-Object)
[IO.File]::WriteAllLines($manifest, [string[]]$entries, [Text.UTF8Encoding]::new($false))
$archive = [IO.Compression.ZipFile]::Open($payloadZip, [IO.Compression.ZipArchiveMode]::Create)
try {
    foreach ($entry in $entries) {
        [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
            $archive, $files[$entry], $entry, [IO.Compression.CompressionLevel]::Optimal)
    }
} finally {
    $archive.Dispose()
}

$iconPath = Join-Path $repository 'src-tauri/icons/icon.ico'
if (-not (Test-Path -LiteralPath $iconPath)) {
    $parentIcon = Join-Path $repository '../../src-tauri/icons/icon.ico'
    if (Test-Path -LiteralPath $parentIcon) { $iconPath = (Resolve-Path -LiteralPath $parentIcon).Path }
}

$installerCs = Join-Path $staging 'Installer.cs'
$csSource = @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.IO;
using System.IO.Compression;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Threading;
using System.Windows.Forms;
using Microsoft.Win32;

[assembly: AssemblyTitle("Instalador de OpenClient")]
[assembly: AssemblyDescription("OpenClient para Minecraft: Java Edition")]
[assembly: AssemblyCompany("OpenClient")]
[assembly: AssemblyProduct("OpenClient")]
[assembly: AssemblyVersion("0.1.0.27")]

namespace OpenClientSetup {
    internal static class Program {
        [DllImport("user32.dll")]
        private static extern bool SetProcessDPIAware();

        [STAThread]
        private static int Main(string[] args) {
            try { SetProcessDPIAware(); } catch { }
            string target = InstallerEngine.DefaultDirectory;
            bool silent = false;
            bool portable = false;
            foreach (string arg in args) {
                if (arg == "--silent" || arg == "/S") silent = true;
                else if (arg == "--portable") portable = true;
                else if (arg.StartsWith("--dir=", StringComparison.Ordinal)) target = arg.Substring(6);
            }
            if (silent) {
                try {
                    InstallerEngine.Install(target, false, false, !portable, null);
                    return 0;
                } catch (Exception ex) {
                    Console.Error.WriteLine(ex.Message);
                    return 1;
                }
            }
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            Application.Run(new SetupForm());
            return 0;
        }
    }

    internal static class InstallerEngine {
        private const string Launcher = "bin/openclient-qt.exe";
        internal const string Uninstaller = "Desinstalar-OpenClient.exe";
        private const string PortableMarker = ".openclient-portable";
        private const string RegistryPath = @"Software\Microsoft\Windows\CurrentVersion\Uninstall\OpenClient";

        internal static string DefaultDirectory {
            get { return Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Programs", "OpenClient"); }
        }

        private static string Target(string path) {
            path = path.Replace('/', '\\');
            if (path.Length < 4 || path[1] != ':' || path[2] != '\\')
                throw new IOException("Selecciona una carpeta dedicada a OpenClient en una unidad local.");
            string full = Path.GetFullPath(path).TrimEnd('\\');
            if (full.Length < 4) throw new IOException("Selecciona una carpeta dedicada a OpenClient.");
            for (DirectoryInfo dir = new DirectoryInfo(full); dir != null; dir = dir.Parent)
                if (dir.Exists && (dir.Attributes & FileAttributes.ReparsePoint) != 0)
                    throw new IOException("La ruta de instalación contiene un enlace o una unión de Windows.");
            return full;
        }

        private static string PackagePath(string target, string relative) {
            foreach (string segment in relative.Split('/'))
                if (segment.Length == 0 || segment.EndsWith(".") || segment.EndsWith(" ") ||
                    segment.IndexOfAny(Path.GetInvalidFileNameChars()) >= 0)
                    throw new InvalidDataException("El paquete contiene una ruta inválida.");
            string path = Path.GetFullPath(Path.Combine(target, relative.Replace('/', '\\')));
            if (!path.StartsWith(target + "\\", StringComparison.OrdinalIgnoreCase))
                throw new InvalidDataException("Una ruta del paquete sale de la carpeta de instalación.");
            for (string current = path; !current.Equals(target, StringComparison.OrdinalIgnoreCase); current = Path.GetDirectoryName(current))
                if ((File.Exists(current) || Directory.Exists(current)) && (File.GetAttributes(current) & FileAttributes.ReparsePoint) != 0)
                    throw new IOException("La instalación contiene un enlace o una unión de Windows.");
            return path;
        }

        private static void EnsureClosed(string exe) {
            foreach (Process process in Process.GetProcessesByName("openclient-qt")) {
                using (process) {
                    string running;
                    try { running = process.MainModule.FileName; }
                    catch (Win32Exception) { continue; }
                    catch (InvalidOperationException) { continue; }
                    if (string.Equals(running, exe, StringComparison.OrdinalIgnoreCase))
                        throw new IOException("Cierra OpenClient y vuelve a intentarlo.");
                }
            }
        }

        private static void WithLock(Action action) {
            using (var mutex = new Mutex(false, @"Local\OpenClientSetup")) {
                bool acquired;
                try { acquired = mutex.WaitOne(0); }
                catch (AbandonedMutexException) { acquired = true; }
                if (!acquired) throw new IOException("Hay otra instalación o desinstalación de OpenClient en curso.");
                try { action(); }
                finally { mutex.ReleaseMutex(); }
            }
        }

        private static bool LooksInstalled(string dir) {
            return File.Exists(Path.Combine(dir, @"bin\openclient-qt.exe")) || File.Exists(Path.Combine(dir, Uninstaller)) ||
                File.Exists(Path.Combine(dir, "Desinstalar-OpenClient.bat"));
        }

        // No sigue enlaces ni uniones: los borra como entrada, sin entrar en ellos.
        private static void DeleteTree(string dir) {
            foreach (string file in Directory.GetFiles(dir)) {
                File.SetAttributes(file, FileAttributes.Normal);
                File.Delete(file);
            }
            foreach (string sub in Directory.GetDirectories(dir)) {
                if ((File.GetAttributes(sub) & FileAttributes.ReparsePoint) != 0) Directory.Delete(sub, false);
                else DeleteTree(sub);
            }
            Directory.Delete(dir, false);
        }

        // Si un archivo sigue abierto (antivirus, indexador), se deja para la próxima instalación.
        private static void TryDeleteTree(string dir) {
            try { if (Directory.Exists(dir)) DeleteTree(dir); }
            catch (IOException) { }
            catch (UnauthorizedAccessException) { }
        }

        // Respaldos `.previous-<guid>` y temporales `.stage-<guid>` de instalaciones anteriores.
        private static void RemoveLeftovers(string target) {
            string parent = Path.GetDirectoryName(target);
            string name = Path.GetFileName(target);
            if (parent == null || !Directory.Exists(parent)) return;
            foreach (string dir in Directory.GetDirectories(parent, name + ".*")) {
                string folder = Path.GetFileName(dir);
                if (!folder.StartsWith(name + ".", StringComparison.OrdinalIgnoreCase)) continue;
                string suffix = folder.Substring(name.Length);
                bool backup = suffix.StartsWith(".previous-", StringComparison.Ordinal);
                bool stage = suffix.StartsWith(".stage-", StringComparison.Ordinal);
                string id = backup ? suffix.Substring(10) : stage ? suffix.Substring(7) : "";
                Guid parsed;
                if (id.Length != 32 || !Guid.TryParseExact(id, "N", out parsed)) continue;
                if ((File.GetAttributes(dir) & FileAttributes.ReparsePoint) != 0) continue;
                if (backup && !LooksInstalled(dir)) continue;
                TryDeleteTree(dir);
            }
        }

        public static void Install(string targetDir, bool desktopShortcut, bool startMenuShortcut, bool register, Action<int, string> onProgress) {
            WithLock(() => {
                string target = Target(targetDir);
                string exe = PackagePath(target, Launcher);
                EnsureClosed(exe);
                RemoveLeftovers(target);
                bool exists = Directory.Exists(target);
                bool update = exists && Directory.GetFileSystemEntries(target).Length > 0;
                if (update && (!File.Exists(exe) || !File.Exists(PackagePath(target, "bin/openclient_ffi.dll")) ||
                    (!File.Exists(PackagePath(target, Uninstaller)) && !File.Exists(PackagePath(target, "Desinstalar-OpenClient.bat")))))
                    throw new IOException("Usa una carpeta vacía o una instalación anterior de OpenClient.");

                string nonce = Guid.NewGuid().ToString("N");
                string staging = target + ".stage-" + nonce;
                string backup = target + ".previous-" + nonce;
                Directory.CreateDirectory(staging);
                try {
                    using (Stream resource = Assembly.GetExecutingAssembly().GetManifestResourceStream("payload.zip"))
                    using (ZipArchive archive = new ZipArchive(resource, ZipArchiveMode.Read)) {
                        int completed = 0;
                        foreach (ZipArchiveEntry entry in archive.Entries) {
                            string destination = PackagePath(staging, entry.FullName);
                            Directory.CreateDirectory(Path.GetDirectoryName(destination));
                            entry.ExtractToFile(destination, false);
                            if (onProgress != null) onProgress(++completed * 95 / archive.Entries.Count, entry.Name);
                        }
                    }
                    using (Stream resource = Assembly.GetExecutingAssembly().GetManifestResourceStream("uninstaller.exe"))
                    using (FileStream output = File.Create(PackagePath(staging, Uninstaller))) resource.CopyTo(output);
                    if (!register) File.WriteAllText(PackagePath(staging, PortableMarker), "");

                    if (update) Directory.Move(target, backup);
                    else if (exists) Directory.Delete(target, false);
                    try { Directory.Move(staging, target); }
                    catch {
                        if (update) Directory.Move(backup, target);
                        throw;
                    }
                    if (update) TryDeleteTree(backup);
                } finally {
                    if (Directory.Exists(staging)) Directory.Delete(staging, true);
                }

                if (register) {
                    using (RegistryKey key = Registry.CurrentUser.CreateSubKey(RegistryPath)) {
                        key.SetValue("DisplayName", "OpenClient");
                        key.SetValue("DisplayVersion", "0.1.0-preview.27");
                        key.SetValue("Publisher", "OpenClient");
                        key.SetValue("DisplayIcon", exe);
                        key.SetValue("InstallLocation", target);
                        key.SetValue("UninstallString", "\"" + PackagePath(target, Uninstaller) + "\"");
                        key.SetValue("NoModify", 1, RegistryValueKind.DWord);
                        key.SetValue("NoRepair", 1, RegistryValueKind.DWord);
                    }
                    if (desktopShortcut) CreateShortcut(Shortcut(Environment.SpecialFolder.DesktopDirectory), exe, target);
                    if (startMenuShortcut) CreateShortcut(Shortcut(Environment.SpecialFolder.Programs), exe, target);
                }
                if (onProgress != null) onProgress(100, "Listo");
            });
        }

        private static string Shortcut(Environment.SpecialFolder folder) {
            return Path.Combine(Environment.GetFolderPath(folder), "OpenClient.lnk");
        }

        private static void RemoveShortcut(string path, string exe) {
            if (!File.Exists(path)) return;
            Type shellType = Type.GetTypeFromProgID("WScript.Shell");
            if (shellType == null) return;
            object shell = Activator.CreateInstance(shellType);
            object shortcut = shellType.InvokeMember("CreateShortcut", BindingFlags.InvokeMethod, null, shell, new object[] { path });
            string current = shortcut.GetType().InvokeMember("TargetPath", BindingFlags.GetProperty, null, shortcut, null) as string;
            if (string.Equals(current, exe, StringComparison.OrdinalIgnoreCase)) File.Delete(path);
        }

        internal static void Remove(string targetDir) {
            WithLock(() => {
                string target = Target(targetDir);
                string exe = PackagePath(target, Launcher);
                string uninstall = PackagePath(target, Uninstaller);
                if (!File.Exists(uninstall)) throw new IOException("No se encontró la instalación de OpenClient.");
                EnsureClosed(exe);
                var files = new List<string>();
                var directories = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
                using (Stream resource = Assembly.GetExecutingAssembly().GetManifestResourceStream("payload.list"))
                using (var reader = new StreamReader(resource)) {
                    string entry;
                    while ((entry = reader.ReadLine()) != null) {
                        string path = PackagePath(target, entry);
                        files.Add(path);
                        for (string dir = Path.GetDirectoryName(path); dir != target; dir = Path.GetDirectoryName(dir))
                            directories.Add(dir);
                    }
                }
                foreach (string file in files)
                    if (File.Exists(file)) File.Delete(file);
                string marker = PackagePath(target, PortableMarker);
                bool registered = false;
                if (File.Exists(marker)) {
                    File.Delete(marker);
                } else {
                    using (RegistryKey key = Registry.CurrentUser.OpenSubKey(RegistryPath))
                        registered = key != null && string.Equals(key.GetValue("InstallLocation") as string, target, StringComparison.OrdinalIgnoreCase);
                }
                if (registered) {
                    RemoveShortcut(Shortcut(Environment.SpecialFolder.DesktopDirectory), exe);
                    RemoveShortcut(Shortcut(Environment.SpecialFolder.Programs), exe);
                    Registry.CurrentUser.DeleteSubKeyTree(RegistryPath, false);
                }
                File.Delete(uninstall);
                var ordered = new List<string>(directories);
                ordered.Sort((a, b) => b.Length.CompareTo(a.Length));
                foreach (string dir in ordered)
                    if (Directory.Exists(dir) && Directory.GetFileSystemEntries(dir).Length == 0) Directory.Delete(dir, false);
                if (Directory.GetFileSystemEntries(target).Length == 0) Directory.Delete(target, false);
            });
        }

        private static void CreateShortcut(string lnkPath, string targetExe, string workDir) {
            try {
                Type shellType = Type.GetTypeFromProgID("WScript.Shell");
                if (shellType == null) return;
                object shell = Activator.CreateInstance(shellType);
                object shortcut = shellType.InvokeMember("CreateShortcut", BindingFlags.InvokeMethod, null, shell, new object[] { lnkPath });
                Type scType = shortcut.GetType();
                scType.InvokeMember("TargetPath", BindingFlags.SetProperty, null, shortcut, new object[] { targetExe });
                scType.InvokeMember("WorkingDirectory", BindingFlags.SetProperty, null, shortcut, new object[] { workDir });
                scType.InvokeMember("Description", BindingFlags.SetProperty, null, shortcut, new object[] { "OpenClient - Launcher Nativo de Minecraft Java" });
                scType.InvokeMember("IconLocation", BindingFlags.SetProperty, null, shortcut, new object[] { targetExe + ",0" });
                scType.InvokeMember("Save", BindingFlags.InvokeMethod, null, shortcut, null);
            } catch { }
        }
    }

    internal static class UninstallProgram {
        [STAThread]
        private static int Main(string[] args) {
            bool silent = Array.IndexOf(args, "--silent") >= 0;
            try {
                if (args.Length >= 3 && args[0] == "--worker") {
                    Process parent;
                    try { parent = Process.GetProcessById(int.Parse(args[1])); }
                    catch (ArgumentException) { parent = null; }
                    using (parent) {
                        if (parent != null && !parent.WaitForExit(15000))
                            throw new IOException("Cierra el desinstalador y vuelve a intentarlo.");
                    }
                    InstallerEngine.Remove(args[2]);
                    if (!silent) MessageBox.Show("OpenClient se ha desinstalado. Tus instancias y skins se conservaron.", "OpenClient");
                    return 0;
                }
                if (!silent && MessageBox.Show("¿Desinstalar OpenClient? Tus instancias y skins se conservarán.",
                    "OpenClient", MessageBoxButtons.OKCancel, MessageBoxIcon.Question) != DialogResult.OK) return 0;
                string current = Assembly.GetExecutingAssembly().Location;
                string helper = Path.Combine(Path.GetTempPath(), "OpenClient-Uninstall-" + Guid.NewGuid().ToString("N") + ".exe");
                File.Copy(current, helper);
                using (Process self = Process.GetCurrentProcess()) {
                    Process.Start(new ProcessStartInfo(helper) {
                        UseShellExecute = false,
                        CreateNoWindow = true,
                        Arguments = "--worker " + self.Id + " \"" + Path.GetDirectoryName(current) + "\"" + (silent ? " --silent" : "")
                    });
                }
                return 0;
            } catch (Exception ex) {
                if (silent) Console.Error.WriteLine(ex.Message);
                else MessageBox.Show(ex.Message, "OpenClient", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return 1;
            }
        }
    }

    internal static class Ui {
        internal static readonly Color Base = Color.FromArgb(13, 11, 13);
        internal static readonly Color Surface = Color.FromArgb(22, 19, 21);
        internal static readonly Color Hairline = Color.FromArgb(24, 255, 255, 255);
        internal static readonly Color HairlineStrong = Color.FromArgb(48, 255, 255, 255);
        internal static readonly Color Accent = Color.FromArgb(217, 38, 78);
        internal static readonly Color AccentHover = Color.FromArgb(230, 56, 94);
        internal static readonly Color AccentPressed = Color.FromArgb(188, 30, 64);
        internal static readonly Color Ink = Color.FromArgb(245, 242, 243);
        internal static readonly Color Dim = Color.FromArgb(163, 154, 158);
        internal static readonly Color Faint = Color.FromArgb(112, 104, 108);
        internal static readonly Color Success = Color.FromArgb(63, 184, 127);
        internal static readonly Color Danger = Color.FromArgb(236, 96, 88);

        private static Font Pick(string[] families, float size, FontStyle style) {
            foreach (string name in families) {
                using (Font probe = new Font(name, size, style)) {
                    if (string.Equals(probe.Name, name, StringComparison.OrdinalIgnoreCase)) return new Font(name, size, style);
                }
            }
            return new Font("Segoe UI", size, style);
        }
        internal static Font Display(float size) { return Pick(new[] { "Segoe UI Semibold", "Segoe UI" }, size, FontStyle.Regular); }
        internal static Font Text(float size, FontStyle style) { return Pick(new[] { "Segoe UI Variable Text", "Segoe UI" }, size, style); }
        internal static Font Mono(float size) { return Pick(new[] { "Cascadia Mono", "Consolas" }, size, FontStyle.Regular); }

        internal static GraphicsPath Round(RectangleF r, float radius) {
            float d = radius * 2;
            GraphicsPath path = new GraphicsPath();
            path.AddArc(r.X, r.Y, d, d, 180, 90);
            path.AddArc(r.Right - d, r.Y, d, d, 270, 90);
            path.AddArc(r.Right - d, r.Bottom - d, d, d, 0, 90);
            path.AddArc(r.X, r.Bottom - d, d, d, 90, 90);
            path.CloseFigure();
            return path;
        }

    }

    internal sealed class FlatButton : Control {
        private readonly bool primary;
        private bool hover;
        private bool pressed;

        public FlatButton(string text, bool primary) {
            this.primary = primary;
            Text = text;
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer | ControlStyles.UserPaint |
                     ControlStyles.ResizeRedraw | ControlStyles.Selectable | ControlStyles.StandardClick, true);
            TabStop = true;
            Cursor = Cursors.Hand;
            Size = new Size(128, 38);
            Font = Ui.Text(9.75f, FontStyle.Bold);
            AccessibleRole = AccessibleRole.PushButton;
        }

        protected override void OnTextChanged(EventArgs e) { AccessibleName = Text; Invalidate(); base.OnTextChanged(e); }
        protected override void OnMouseEnter(EventArgs e) { hover = true; Invalidate(); base.OnMouseEnter(e); }
        protected override void OnMouseLeave(EventArgs e) { hover = false; pressed = false; Invalidate(); base.OnMouseLeave(e); }
        protected override void OnMouseDown(MouseEventArgs e) {
            if (e.Button == MouseButtons.Left) { pressed = true; Focus(); Invalidate(); }
            base.OnMouseDown(e);
        }
        protected override void OnMouseUp(MouseEventArgs e) { pressed = false; Invalidate(); base.OnMouseUp(e); }
        protected override void OnGotFocus(EventArgs e) { Invalidate(); base.OnGotFocus(e); }
        protected override void OnLostFocus(EventArgs e) { Invalidate(); base.OnLostFocus(e); }
        protected override void OnEnabledChanged(EventArgs e) { Cursor = Enabled ? Cursors.Hand : Cursors.Default; Invalidate(); base.OnEnabledChanged(e); }
        protected override bool IsInputKey(Keys keyData) { return keyData == Keys.Space || keyData == Keys.Enter || base.IsInputKey(keyData); }
        protected override void OnKeyDown(KeyEventArgs e) {
            if (Enabled && (e.KeyCode == Keys.Enter || e.KeyCode == Keys.Space)) { OnClick(EventArgs.Empty); e.Handled = true; }
            base.OnKeyDown(e);
        }

        protected override void OnPaint(PaintEventArgs e) {
            Graphics g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.Clear(Parent != null ? Parent.BackColor : Ui.Base);
            RectangleF box = new RectangleF(0.5f, 0.5f, Width - 1.5f, Height - 1.5f);
            if (pressed) box.Inflate(-1, -1);
            Color fill;
            if (!Enabled) fill = primary ? Color.FromArgb(70, Ui.Accent) : Color.Transparent;
            else if (primary) fill = pressed ? Ui.AccentPressed : hover ? Ui.AccentHover : Ui.Accent;
            else fill = pressed ? Color.FromArgb(34, 255, 255, 255) : hover ? Color.FromArgb(18, 255, 255, 255) : Color.Transparent;
            using (GraphicsPath path = Ui.Round(box, 8)) {
                if (fill.A > 0) using (Brush b = new SolidBrush(fill)) g.FillPath(b, path);
                if (!primary) using (Pen p = new Pen(hover && Enabled ? Ui.HairlineStrong : Ui.Hairline)) g.DrawPath(p, path);
            }
            if (Focused && ShowFocusCues) {
                RectangleF ring = box;
                ring.Inflate(-3, -3);
                using (GraphicsPath path = Ui.Round(ring, 6))
                using (Pen p = new Pen(Color.FromArgb(170, 255, 255, 255), 1.2f)) g.DrawPath(p, path);
            }
            Color ink = !Enabled ? Ui.Faint : primary ? Color.White : Ui.Ink;
            TextRenderer.DrawText(g, Text, Font, Rectangle.Round(box), ink,
                TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter | TextFormatFlags.SingleLine | TextFormatFlags.EndEllipsis);
        }
    }

    internal sealed class CheckRow : Control {
        private bool isChecked = true;
        private bool hover;

        public CheckRow(string text) {
            Text = text;
            AccessibleName = text;
            AccessibleRole = AccessibleRole.CheckButton;
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer | ControlStyles.UserPaint |
                     ControlStyles.ResizeRedraw | ControlStyles.Selectable | ControlStyles.StandardClick, true);
            TabStop = true;
            Cursor = Cursors.Hand;
            Size = new Size(320, 30);
            Font = Ui.Text(9.75f, FontStyle.Regular);
        }

        public bool Checked {
            get { return isChecked; }
            set { isChecked = value; AccessibleDescription = value ? "Activado" : "Desactivado"; Invalidate(); }
        }

        protected override void OnClick(EventArgs e) { if (Enabled) Checked = !Checked; base.OnClick(e); }
        protected override void OnMouseEnter(EventArgs e) { hover = true; Invalidate(); base.OnMouseEnter(e); }
        protected override void OnMouseLeave(EventArgs e) { hover = false; Invalidate(); base.OnMouseLeave(e); }
        protected override void OnGotFocus(EventArgs e) { Invalidate(); base.OnGotFocus(e); }
        protected override void OnLostFocus(EventArgs e) { Invalidate(); base.OnLostFocus(e); }
        protected override void OnEnabledChanged(EventArgs e) { Invalidate(); base.OnEnabledChanged(e); }
        protected override bool IsInputKey(Keys keyData) { return keyData == Keys.Space || base.IsInputKey(keyData); }
        protected override void OnKeyDown(KeyEventArgs e) {
            if (Enabled && e.KeyCode == Keys.Space) { Checked = !Checked; e.Handled = true; }
            base.OnKeyDown(e);
        }

        protected override void OnPaint(PaintEventArgs e) {
            Graphics g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.Clear(Parent != null ? Parent.BackColor : Ui.Base);
            RectangleF box = new RectangleF(1.5f, (Height - 18) / 2f, 17, 17);
            using (GraphicsPath path = Ui.Round(box, 4.5f)) {
                if (isChecked) {
                    using (Brush b = new SolidBrush(Enabled ? (hover ? Ui.AccentHover : Ui.Accent) : Color.FromArgb(70, Ui.Accent))) g.FillPath(b, path);
                    using (Pen tick = new Pen(Color.White, 1.8f)) {
                        tick.StartCap = LineCap.Round;
                        tick.EndCap = LineCap.Round;
                        tick.LineJoin = LineJoin.Round;
                        g.DrawLines(tick, new[] {
                            new PointF(box.X + 4.2f, box.Y + 8.8f), new PointF(box.X + 7.3f, box.Y + 11.8f), new PointF(box.X + 12.8f, box.Y + 5.4f) });
                    }
                } else {
                    using (Pen p = new Pen(hover ? Color.FromArgb(110, 255, 255, 255) : Color.FromArgb(70, 255, 255, 255), 1.2f)) g.DrawPath(p, path);
                }
            }
            if (Focused && ShowFocusCues) {
                RectangleF ring = box;
                ring.Inflate(3, 3);
                using (GraphicsPath path = Ui.Round(ring, 6.5f))
                using (Pen p = new Pen(Color.FromArgb(150, 255, 255, 255), 1.2f)) g.DrawPath(p, path);
            }
            TextRenderer.DrawText(g, Text, Font, new Rectangle(28, 0, Width - 28, Height), Enabled ? Ui.Ink : Ui.Faint,
                TextFormatFlags.VerticalCenter | TextFormatFlags.SingleLine | TextFormatFlags.EndEllipsis);
        }
    }

    internal sealed class ProgressLine : Control {
        private float shown;
        private float target;
        private readonly System.Windows.Forms.Timer ease = new System.Windows.Forms.Timer { Interval = 16 };

        public ProgressLine() {
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer | ControlStyles.UserPaint | ControlStyles.ResizeRedraw, true);
            Height = 4;
            ease.Tick += (s, e) => {
                // Deceleración exponencial: avanza una fracción fija de lo que falta en cada fotograma.
                shown += (target - shown) * 0.22f;
                if (Math.Abs(target - shown) < 0.002f) { shown = target; ease.Stop(); }
                Invalidate();
            };
        }

        public float Value {
            get { return target; }
            set {
                target = Math.Max(0f, Math.Min(1f, value));
                if (!SystemInformation.UIEffectsEnabled) { shown = target; Invalidate(); }
                else if (!ease.Enabled) ease.Start();
            }
        }

        protected override void Dispose(bool disposing) { if (disposing) ease.Dispose(); base.Dispose(disposing); }

        protected override void OnPaint(PaintEventArgs e) {
            Graphics g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.Clear(Parent != null ? Parent.BackColor : Ui.Base);
            float radius = Height / 2f;
            using (GraphicsPath track = Ui.Round(new RectangleF(0, 0, Width - 1, Height - 0.5f), radius))
            using (Brush b = new SolidBrush(Color.FromArgb(28, 255, 255, 255))) g.FillPath(b, track);
            float width = (Width - 1) * shown;
            if (width >= Height) {
                using (GraphicsPath fill = Ui.Round(new RectangleF(0, 0, width, Height - 0.5f), radius))
                using (Brush b = new SolidBrush(Ui.Accent)) g.FillPath(b, fill);
            }
        }
    }

    internal sealed class SetupForm : Form {
        [DllImport("user32.dll")] private static extern bool ReleaseCapture();
        [DllImport("user32.dll")] private static extern IntPtr SendMessage(IntPtr hWnd, int Msg, int wParam, int lParam);
        [DllImport("gdi32.dll")] private static extern bool DeleteObject(IntPtr handle);
        [DllImport("gdi32.dll")] private static extern IntPtr CreateRoundRectRgn(int x1, int y1, int x2, int y2, int cx, int cy);

        private const int Ready = 0, Installing = 1, Done = 2, Failed = 3;
        private const int Gutter = 36;
        private int state = Ready;
        private string targetPath = InstallerEngine.DefaultDirectory;
        private string installError = "";
        private bool wasUpdate;

        private readonly Label title;
        private readonly Label subtitle;
        private readonly Panel content;
        private readonly Label footnote;
        private readonly FlatButton primaryButton;
        private readonly FlatButton secondaryButton;
        private readonly CheckRow desktop;
        private readonly CheckRow startMenu;
        private readonly Label pathText;
        private readonly FlatButton changeButton;
        private readonly Label statusText;
        private readonly Label percentText;
        private readonly Label fileText;
        private readonly ProgressLine progress;
        private readonly System.Windows.Forms.Timer fade = new System.Windows.Forms.Timer { Interval = 15 };
        private DateTime fadeStart;

        public SetupForm() {
            Text = "Instalar OpenClient";
            AutoScaleDimensions = new SizeF(96, 96);
            AutoScaleMode = AutoScaleMode.Dpi;
            ClientSize = new Size(560, 404);
            StartPosition = FormStartPosition.CenterScreen;
            FormBorderStyle = FormBorderStyle.None;
            BackColor = Ui.Base;
            ForeColor = Ui.Ink;
            DoubleBuffered = true;
            Font = Ui.Text(9.75f, FontStyle.Regular);
            KeyPreview = true;
            try { Icon = Icon.ExtractAssociatedIcon(Assembly.GetExecutingAssembly().Location); } catch { }
            RoundCorners();
            Resize += (s, e) => RoundCorners();
            MouseDown += DragWindow;

            Control close = new CloseGlyph();
            close.Location = new Point(ClientSize.Width - 48, 14);
            close.Click += (s, e) => { if (state != Installing) Close(); };
            Controls.Add(close);

            PictureBox mark = new PictureBox {
                Location = new Point(Gutter, 40),
                Size = new Size(40, 40),
                SizeMode = PictureBoxSizeMode.Zoom,
                Image = Icon == null ? null : new Icon(Icon, 48, 48).ToBitmap()
            };
            mark.MouseDown += DragWindow;
            Controls.Add(mark);

            title = new Label { Location = new Point(Gutter - 2, 96), AutoSize = true, Font = Ui.Display(21f), ForeColor = Ui.Ink };
            subtitle = new Label { Location = new Point(Gutter, 138), Size = new Size(488, 40), Font = Ui.Text(9.75f, FontStyle.Regular), ForeColor = Ui.Dim };
            title.MouseDown += DragWindow;
            subtitle.MouseDown += DragWindow;
            Controls.Add(title);
            Controls.Add(subtitle);

            content = new Panel { Location = new Point(Gutter, 192), Size = new Size(488, 132), BackColor = Ui.Base };
            content.MouseDown += DragWindow;
            Controls.Add(content);

            Panel pathField = new Panel { Location = new Point(0, 22), Size = new Size(384, 38), BackColor = Ui.Base };
            pathField.Paint += (s, e) => {
                e.Graphics.SmoothingMode = SmoothingMode.AntiAlias;
                using (GraphicsPath path = Ui.Round(new RectangleF(0.5f, 0.5f, pathField.Width - 1.5f, pathField.Height - 1.5f), 8)) {
                    using (Brush b = new SolidBrush(Ui.Surface)) e.Graphics.FillPath(b, path);
                    using (Pen p = new Pen(Ui.Hairline)) e.Graphics.DrawPath(p, path);
                }
            };
            pathText = new Label {
                Location = new Point(12, 1), Size = new Size(362, 36), AutoEllipsis = true, TextAlign = ContentAlignment.MiddleLeft,
                Font = Ui.Text(9.25f, FontStyle.Regular), ForeColor = Ui.Ink, BackColor = Ui.Surface, UseMnemonic = false
            };
            pathField.Controls.Add(pathText);
            changeButton = new FlatButton("Cambiar…", false) { Location = new Point(392, 22), Size = new Size(96, 38) };
            changeButton.Click += (s, e) => ChooseFolder();
            desktop = new CheckRow("Acceso directo en el Escritorio") { Location = new Point(0, 74) };
            startMenu = new CheckRow("Acceso en el menú Inicio") { Location = new Point(0, 102) };

            statusText = new Label { Location = new Point(0, 0), Size = new Size(400, 22), Font = Ui.Text(9.75f, FontStyle.Regular), ForeColor = Ui.Ink };
            percentText = new Label { Location = new Point(400, 0), Size = new Size(88, 22), TextAlign = ContentAlignment.TopRight, Font = Ui.Mono(9.5f), ForeColor = Ui.Dim };
            progress = new ProgressLine { Location = new Point(0, 32), Size = new Size(488, 4) };
            fileText = new Label { Location = new Point(0, 48), Size = new Size(488, 20), AutoEllipsis = true, Font = Ui.Text(8.5f, FontStyle.Regular), ForeColor = Ui.Faint, UseMnemonic = false };

            footnote = new Label { Location = new Point(Gutter, 352), Size = new Size(240, 34), Font = Ui.Text(8.25f, FontStyle.Regular), ForeColor = Ui.Faint };
            footnote.MouseDown += DragWindow;
            Controls.Add(footnote);
            secondaryButton = new FlatButton("Cerrar", false) { Location = new Point(ClientSize.Width - Gutter - 128 - 8 - 104, 348), Size = new Size(104, 38) };
            primaryButton = new FlatButton("Instalar", true) { Location = new Point(ClientSize.Width - Gutter - 128, 348), Size = new Size(128, 38) };
            secondaryButton.Click += (s, e) => { if (state == Failed) ShowState(Ready); else Close(); };
            primaryButton.Click += OnPrimary;
            Controls.Add(secondaryButton);
            Controls.Add(primaryButton);

            fade.Tick += (s, e) => {
                double t = Math.Min(1.0, (DateTime.Now - fadeStart).TotalMilliseconds / 220.0);
                Opacity = 1 - Math.Pow(1 - t, 3);
                if (t >= 1) fade.Stop();
            };
            ShowState(Ready);
        }

        protected override void OnShown(EventArgs e) {
            base.OnShown(e);
            primaryButton.Focus();
            if (SystemInformation.UIEffectsEnabled) {
                Opacity = 0;
                fadeStart = DateTime.Now;
                fade.Start();
            }
        }

        protected override void OnKeyDown(KeyEventArgs e) {
            if (e.KeyCode == Keys.Escape && state != Installing) { Close(); e.Handled = true; }
            base.OnKeyDown(e);
        }

        private static bool IsExistingInstall(string path) {
            try { return File.Exists(Path.Combine(path, "bin", "openclient-qt.exe")); }
            catch (ArgumentException) { return false; }
        }

        private void ShowState(int next) {
            state = next;
            bool update = IsExistingInstall(targetPath);
            content.Controls.Clear();
            secondaryButton.Visible = false;
            primaryButton.Visible = true;
            primaryButton.Enabled = true;

            if (state == Ready) {
                title.Text = update ? "Actualizar OpenClient" : "Instalar OpenClient";
                title.ForeColor = Ui.Ink;
                subtitle.Text = update
                    ? "Ya hay una versión en esta carpeta. Se reemplaza el programa; tus instancias, mundos y ajustes se quedan como están."
                    : "Launcher para Minecraft: Java Edition. Ocupa unos 200 MB y se instala solo para tu usuario.";
                pathText.Text = targetPath;
                content.Controls.AddRange(new Control[] {
                    new Label { Text = "Ubicación", Location = new Point(0, 0), AutoSize = true, Font = Ui.Text(8.5f, FontStyle.Regular), ForeColor = Ui.Faint },
                    pathText.Parent, changeButton, desktop, startMenu });
                primaryButton.Text = update ? "Actualizar" : "Instalar";
                footnote.Text = "0.1.0-preview.27 · Windows x64";
            } else if (state == Installing) {
                wasUpdate = update;
                title.Text = update ? "Actualizando…" : "Instalando…";
                subtitle.Text = "Copiando los archivos del programa. Tarda menos de un minuto.";
                statusText.Text = "Preparando archivos";
                percentText.Text = "0 %";
                fileText.Text = "";
                progress.Value = 0;
                content.Controls.AddRange(new Control[] { statusText, percentText, progress, fileText });
                primaryButton.Visible = false;
                footnote.Text = "No cierres esta ventana.";
            } else if (state == Done) {
                title.Text = "Listo";
                subtitle.Text = wasUpdate ? "OpenClient se actualizó." : "OpenClient quedó instalado.";
                Label where = new Label {
                    Text = targetPath, Location = new Point(28, 0), Size = new Size(460, 18), AutoEllipsis = true,
                    TextAlign = ContentAlignment.MiddleLeft, Font = Ui.Text(9.25f, FontStyle.Regular), ForeColor = Ui.Dim, UseMnemonic = false
                };
                content.Controls.Add(new StatusDot(Ui.Success) { Location = new Point(0, 0) });
                content.Controls.Add(where);
                primaryButton.Text = "Abrir OpenClient";
                secondaryButton.Text = "Cerrar";
                secondaryButton.Visible = true;
                footnote.Text = "0.1.0-preview.27 · Windows x64";
            } else {
                title.Text = "No se pudo instalar";
                title.ForeColor = Ui.Ink;
                subtitle.Text = "Nada de tu instalación anterior se perdió.";
                content.Controls.Add(new StatusDot(Ui.Danger) { Location = new Point(0, 0) });
                content.Controls.Add(new Label {
                    Text = installError, Location = new Point(28, 0), Size = new Size(460, 80),
                    Font = Ui.Text(9.5f, FontStyle.Regular), ForeColor = Ui.Danger, UseMnemonic = false
                });
                primaryButton.Text = "Reintentar";
                secondaryButton.Text = "Cambiar ruta";
                secondaryButton.Visible = true;
                footnote.Text = "";
            }
            Invalidate();
        }

        private void ChooseFolder() {
            using (FolderBrowserDialog dialog = new FolderBrowserDialog()) {
                dialog.Description = "Elige una carpeta vacía o la de una instalación anterior de OpenClient";
                dialog.SelectedPath = targetPath;
                if (dialog.ShowDialog(this) != DialogResult.OK) return;
                targetPath = dialog.SelectedPath;
                ShowState(Ready);
            }
        }

        private void OnPrimary(object sender, EventArgs e) {
            if (state == Ready || state == Failed) {
                ShowState(Installing);
                StartInstall();
            } else if (state == Done) {
                string exe = Path.Combine(targetPath, "bin", "openclient-qt.exe");
                if (File.Exists(exe)) {
                    try { Process.Start(new ProcessStartInfo(exe) { WorkingDirectory = Path.GetDirectoryName(exe) }); } catch { }
                }
                Close();
            }
        }

        private void ReportProgress(int percent, string file) {
            percentText.Text = percent + " %";
            statusText.Text = percent >= 95 ? "Terminando" : "Copiando archivos";
            fileText.Text = file;
            progress.Value = percent / 100f;
        }

        private void StartInstall() {
            string target = targetPath.Trim();
            bool makeDesktop = desktop.Checked;
            bool makeStart = startMenu.Checked;
            Thread worker = new Thread(() => {
                try {
                    InstallerEngine.Install(target, makeDesktop, makeStart, true, (percent, name) => {
                        try { BeginInvoke((Action)(() => ReportProgress(percent, name))); } catch { }
                    });
                    installError = "";
                } catch (Exception ex) {
                    installError = ex.Message;
                }
                try { BeginInvoke((Action)(() => ShowState(string.IsNullOrEmpty(installError) ? Done : Failed))); } catch { }
            });
            worker.IsBackground = true;
            worker.SetApartmentState(ApartmentState.STA);
            worker.Start();
        }

        private void RoundCorners() {
            IntPtr handle = CreateRoundRectRgn(0, 0, Width + 1, Height + 1, 20, 20);
            if (handle == IntPtr.Zero) return;
            try {
                Region previous = Region;
                Region = Region.FromHrgn(handle);
                if (previous != null) previous.Dispose();
            } finally { DeleteObject(handle); }
        }

        protected override void OnFormClosing(FormClosingEventArgs e) {
            if (state == Installing && e.CloseReason == CloseReason.UserClosing) e.Cancel = true;
            base.OnFormClosing(e);
        }

        protected override void OnPaint(PaintEventArgs e) {
            base.OnPaint(e);
            e.Graphics.SmoothingMode = SmoothingMode.AntiAlias;
            using (GraphicsPath edge = Ui.Round(new RectangleF(0.5f, 0.5f, Width - 1.5f, Height - 1.5f), 10))
            using (Pen border = new Pen(Ui.HairlineStrong)) e.Graphics.DrawPath(border, edge);
            using (LinearGradientBrush light = new LinearGradientBrush(new Point(24, 1), new Point(Width - 24, 1),
                       Color.FromArgb(0, 255, 255, 255), Color.FromArgb(0, 255, 255, 255))) {
                ColorBlend blend = new ColorBlend {
                    Colors = new[] { Color.FromArgb(0, 255, 255, 255), Color.FromArgb(34, 255, 255, 255), Color.FromArgb(0, 255, 255, 255) },
                    Positions = new[] { 0f, 0.5f, 1f }
                };
                light.InterpolationColors = blend;
                using (Pen specular = new Pen(light, 1)) e.Graphics.DrawLine(specular, 24, 1, Width - 24, 1);
            }
            using (Pen rule = new Pen(Ui.Hairline)) e.Graphics.DrawLine(rule, Gutter, 334, Width - Gutter, 334);
        }

        private void DragWindow(object sender, MouseEventArgs e) {
            if (e.Button != MouseButtons.Left) return;
            ReleaseCapture();
            SendMessage(Handle, 0xA1, 0x2, 0);
        }
    }

    internal sealed class StatusDot : Control {
        private readonly Color tone;
        public StatusDot(Color tone) { this.tone = tone; SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer | ControlStyles.UserPaint | ControlStyles.ResizeRedraw, true); Size = new Size(18, 18); }
        protected override void OnPaint(PaintEventArgs e) {
            Graphics g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.Clear(Parent != null ? Parent.BackColor : Ui.Base);
            using (Brush halo = new SolidBrush(Color.FromArgb(40, tone))) g.FillEllipse(halo, 0, 0, 17, 17);
            using (Brush core = new SolidBrush(tone)) g.FillEllipse(core, 5, 5, 7, 7);
        }
    }

    internal sealed class CloseGlyph : Control {
        private bool hover;
        public CloseGlyph() {
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer | ControlStyles.UserPaint |
                     ControlStyles.Selectable | ControlStyles.StandardClick, true);
            Size = new Size(32, 32);
            Cursor = Cursors.Hand;
            TabStop = false;
            AccessibleName = "Cerrar";
            AccessibleRole = AccessibleRole.PushButton;
        }
        protected override void OnMouseEnter(EventArgs e) { hover = true; Invalidate(); base.OnMouseEnter(e); }
        protected override void OnMouseLeave(EventArgs e) { hover = false; Invalidate(); base.OnMouseLeave(e); }
        protected override void OnPaint(PaintEventArgs e) {
            Graphics g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.Clear(Parent != null ? Parent.BackColor : Ui.Base);
            if (hover) using (GraphicsPath path = Ui.Round(new RectangleF(0.5f, 0.5f, Width - 1.5f, Height - 1.5f), 8))
                using (Brush b = new SolidBrush(Color.FromArgb(20, 255, 255, 255))) g.FillPath(b, path);
            using (Pen p = new Pen(hover ? Ui.Ink : Ui.Dim, 1.4f)) {
                p.StartCap = LineCap.Round;
                p.EndCap = LineCap.Round;
                g.DrawLine(p, 11.5f, 11.5f, 20.5f, 20.5f);
                g.DrawLine(p, 20.5f, 11.5f, 11.5f, 20.5f);
            }
        }
    }
}
'@

Set-Content -LiteralPath $installerCs -Value $csSource -Encoding UTF8

$winRoot = if ($env:SystemRoot) { $env:SystemRoot } else { 'C:\Windows' }
$csc = Join-Path $winRoot 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
if (-not (Test-Path -LiteralPath $csc)) { throw 'No se encontró el compilador nativo csc.exe de .NET Framework.' }

$references = @(
    '/reference:System.dll',
    '/reference:System.Drawing.dll',
    '/reference:System.Windows.Forms.dll',
    '/reference:System.IO.Compression.dll',
    '/reference:System.IO.Compression.FileSystem.dll'
)
$uninstaller = Join-Path $staging 'Desinstalar-OpenClient.exe'
$installer = Join-Path $staging 'Instalar-OpenClient.exe'
$common = @('/nologo', '/target:winexe', '/optimize+', '/codepage:65001') + $references
if (Test-Path -LiteralPath $iconPath) { $common += "/win32icon:$iconPath" }

& $csc @common '/main:OpenClientSetup.UninstallProgram' "/out:$uninstaller" "/resource:$manifest,payload.list" $installerCs
if ($LASTEXITCODE -ne 0) { throw 'Falló la compilación del desinstalador.' }
& $csc @common '/main:OpenClientSetup.Program' "/out:$installer" "/resource:$payloadZip,payload.zip" "/resource:$uninstaller,uninstaller.exe" $installerCs
if ($LASTEXITCODE -ne 0) { throw 'Falló la compilación del instalador.' }

if (Test-Path -LiteralPath $OutputExe) {
    [IO.File]::Replace($installer, $OutputExe, [NullString]::Value)
} else {
    [IO.File]::Move($installer, $OutputExe)
}
Write-Output "Instalador creado: $OutputExe"
} finally {
    if ((Split-Path -Parent $staging) -ne $outputFolder -or (Split-Path -Leaf $staging) -notmatch '^installer-staging-[a-f0-9]{32}$') {
        throw 'No se pudo comprobar la ruta temporal del instalador.'
    }
    Remove-Item -LiteralPath $staging -Recurse -Force
}
