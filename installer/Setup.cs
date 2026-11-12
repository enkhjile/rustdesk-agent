// Built with the Windows .NET Framework compiler. No downloaded setup runtime.
using System;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Reflection;
using System.Security.AccessControl;
using System.Security.Cryptography;
using System.ServiceProcess;
using System.Threading.Tasks;
using System.Windows.Forms;

internal static class Setup
{
    private static readonly string ProgramFiles = Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles);
    private static readonly string ProgramData = Environment.GetFolderPath(Environment.SpecialFolder.CommonApplicationData);
    private static readonly string Agent = Path.Combine(ProgramFiles, @"RustDeskAgent\rustdesk-agent.exe");
    private const string ServiceKey = @"SYSTEM\CurrentControlSet\Services\RustDeskAgent";
    private const int InitializationPending = 1;
    private const int StartupPending = 2;

    [STAThread]
    private static void Main()
    {
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        Application.Run(new SetupWindow());
    }

    private static bool ServiceExists()
    {
        foreach (var service in ServiceController.GetServices())
            using (service) { if (service.ServiceName == "RustDeskAgent") return true; }
        return false;
    }

    // Never include stdin in arguments, files, error messages or logs.
    private static string Run(string executable, string arguments, string input = null)
    {
        var info = new ProcessStartInfo(executable, arguments) {
            UseShellExecute = false, CreateNoWindow = true,
            RedirectStandardInput = true, RedirectStandardOutput = true, RedirectStandardError = true,
            WorkingDirectory = Environment.GetFolderPath(Environment.SpecialFolder.System)
        };
        using (var process = Process.Start(info))
        {
            var output = process.StandardOutput.ReadToEndAsync();
            var error = process.StandardError.ReadToEndAsync();
            if (input != null) process.StandardInput.WriteLine(input);
            process.StandardInput.Close();
            if (!process.WaitForExit(180000)) {
                process.Kill();
                throw new InvalidOperationException("Setup step timed out. Inspect the service before retrying.");
            }
            Task.WaitAll(output, error);
            if (process.ExitCode != 0)
                throw new InvalidOperationException(error.Result.Length == 0 ? "Setup step failed." : error.Result);
            return output.Result;
        }
    }

    internal static string Install(string origin, string credential, bool observeOnly)
    {
        var security = new MutexSecurity();
        security.SetSecurityDescriptorSddlForm("D:P(A;;GA;;;SY)(A;;GA;;;BA)");
        bool created;
        using (var guard = new System.Threading.Mutex(false, @"Global\RustDeskAgentGuidedSetup", out created, security)) {
            if (!created) throw new InvalidOperationException("Another setup is running. Wait for it to finish.");
            return InstallCore(origin, credential, observeOnly);
        }
    }

    internal static bool NeedsCompletion(bool upgrade, bool enrollmentExists, bool stateExists, int pending)
    {
        if (upgrade && !enrollmentExists && pending != InitializationPending)
            throw new InvalidOperationException("Enrollment is missing from an existing installation. Restore it before upgrading; setup will not create a replacement identity.");
        if (!enrollmentExists && stateExists)
            throw new InvalidOperationException("Enrollment is missing but state exists. Restore enrollment before retrying; setup will not replace this device's identity.");
        if (enrollmentExists && !stateExists && pending != InitializationPending)
            throw new InvalidOperationException("Durable state is missing. Restore it before upgrading; setup will not reset this device's history.");
        return !upgrade || pending != 0;
    }

    private static void SetPending(int pending)
    {
        using (var key = Microsoft.Win32.Registry.LocalMachine.OpenSubKey(ServiceKey, true))
            key.SetValue("SetupPending", pending, Microsoft.Win32.RegistryValueKind.DWord);
    }

    private static void CompleteSetup(int pending)
    {
        using (var service = new ServiceController("RustDeskAgent")) {
            if (service.Status != ServiceControllerStatus.Stopped) {
                service.Stop();
                service.WaitForStatus(ServiceControllerStatus.Stopped, TimeSpan.FromSeconds(30));
            }
            Run(Agent, "--finish-setup" + (pending == InitializationPending ? " --initialize-if-missing" : ""));
            SetPending(StartupPending);
            service.Start();
            service.WaitForStatus(ServiceControllerStatus.Running, TimeSpan.FromSeconds(30));
        }
        using (var key = Microsoft.Win32.Registry.LocalMachine.OpenSubKey(ServiceKey, true))
            key.DeleteValue("SetupPending", false);
    }

    private static string InstallCore(string origin, string credential, bool observeOnly)
    {
        bool upgrade = ServiceExists();
        bool enrollmentExists = Directory.Exists(Path.Combine(ProgramData, "RustDeskAgentEnrollment"));
        bool stateExists = Directory.Exists(Path.Combine(ProgramData, "RustDeskAgentState"));
        int pending = 0;
        using (var key = Microsoft.Win32.Registry.LocalMachine.OpenSubKey(ServiceKey))
            if (key != null) pending = Convert.ToInt32(key.GetValue("SetupPending", 0));
        bool finish = NeedsCompletion(upgrade, enrollmentExists, stateExists, pending);
        if (!enrollmentExists) {
            Uri url;
            if (!Uri.TryCreate(origin, UriKind.Absolute, out url) || url.Scheme != "https" ||
                url.UserInfo != "" || url.AbsolutePath != "/" || url.Query != "" || url.Fragment != "")
                throw new InvalidOperationException("Enter the Console HTTPS origin, for example https://console.example.com");
            if (credential.Length < 32 || credential.Length > 256)
                throw new InvalidOperationException("Paste the device credential issued by Console.");
            origin = url.GetLeftPart(UriPartial.Authority);
        }
        // Program Files is not writable by ordinary users; the random staging
        // directory is created with a protected administrator/SYSTEM-only ACL.
        string staging = Path.Combine(ProgramFiles, "RustDeskAgentSetup-" + Guid.NewGuid().ToString("N"));
        var security = new DirectorySecurity();
        security.SetSecurityDescriptorSddlForm("O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)");
        Directory.CreateDirectory(staging, security);
        try {
            foreach (string name in new[] { "rustdesk-agent.exe", "install.ps1" })
                using (var resource = Assembly.GetExecutingAssembly().GetManifestResourceStream(name))
                using (var destination = new FileStream(Path.Combine(staging, name), FileMode.CreateNew))
                    resource.CopyTo(destination);
            string hash;
            using (var sha = SHA256.Create())
            using (var file = File.OpenRead(Path.Combine(staging, "rustdesk-agent.exe")))
                hash = BitConverter.ToString(sha.ComputeHash(file)).Replace("-", "");
            string powershell = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.System), @"WindowsPowerShell\v1.0\powershell.exe");
            Run(powershell, "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"" + Path.Combine(staging, "install.ps1") +
                "\" -Action " + (upgrade ? "Upgrade" : "Install") + " -SourcePath \"" + Path.Combine(staging, "rustdesk-agent.exe") +
                "\" -Sha256 " + hash + (!upgrade && observeOnly ? " -Mode ObserveOnly" : ""));
            if (finish && pending == 0) {
                pending = enrollmentExists ? StartupPending : InitializationPending;
                SetPending(pending);
            }
            if (!enrollmentExists) Run(Agent, "--enroll \"" + origin + "\"", credential);
            if (finish) CompleteSetup(pending);
            using (var service = new ServiceController("RustDeskAgent")) {
                if (service.Status != ServiceControllerStatus.Running)
                    return "Upgrade complete. The service was stopped and remains stopped. Enrollment and state were preserved.";
            }
            Run(Agent, "--wait-for-check-in");
            return "Setup complete. Console has received an agent report. You can now manage this device from Console.";
        }
        finally {
            foreach (string name in new[] { "rustdesk-agent.exe", "install.ps1" }) File.Delete(Path.Combine(staging, name));
            Directory.Delete(staging);
        }
    }

    private sealed class SetupWindow : Form
    {
        private readonly TextBox origin = new TextBox { Width = 450 };
        private readonly TextBox token = new TextBox { Width = 450, UseSystemPasswordChar = true, MaxLength = 256 };
        private readonly CheckBox observe = new CheckBox { Text = "Reporting only (do not apply Console settings)", AutoSize = true };
        private readonly Button install = new Button { Text = "Install", Width = 110, Height = 30 };
        private readonly Label status = new Label { Width = 450, Height = 110 };
        private bool busy;
        internal SetupWindow()
        {
            Text = "RustDesk Agent Setup";
            ClientSize = new Size(490, 380); FormBorderStyle = FormBorderStyle.FixedDialog;
            MaximizeBox = false; StartPosition = FormStartPosition.CenterScreen;
            var panel = new FlowLayoutPanel { Dock = DockStyle.Fill, Padding = new Padding(18), FlowDirection = FlowDirection.TopDown, WrapContents = false };
            panel.Controls.Add(new Label { Text = "Install official RustDesk 1.4.7 first. This installer is unsigned.", Width = 450, Height = 34 });
            panel.Controls.Add(new Label { Text = "Console HTTPS URL", AutoSize = true }); panel.Controls.Add(origin);
            panel.Controls.Add(new Label { Text = "Device token (Console → Device info → Companion agent)", AutoSize = true }); panel.Controls.Add(token);
            panel.Controls.Add(observe); panel.Controls.Add(install); panel.Controls.Add(status); Controls.Add(panel);
            var licenses = new LinkLabel { Text = "License notices", AutoSize = true };
            licenses.LinkClicked += delegate {
                using (var window = new Form { Text = "License notices", Size = new Size(700, 500) }) {
                    var text = new TextBox { Multiline = true, ReadOnly = true, ScrollBars = ScrollBars.Both, Dock = DockStyle.Fill };
                    foreach (var name in new[] { "LICENSE", "THIRD-PARTY-NOTICES.txt", "RUST-STANDARD-LIBRARY-NOTICES.html" })
                        using (var reader = new StreamReader(Assembly.GetExecutingAssembly().GetManifestResourceStream(name)))
                            text.AppendText(name + Environment.NewLine + reader.ReadToEnd() + Environment.NewLine);
                    window.Controls.Add(text); window.ShowDialog(this);
                }
            };
            panel.Controls.Add(licenses);
            bool existing = Directory.Exists(Path.Combine(ProgramData, "RustDeskAgentEnrollment"));
            origin.Enabled = token.Enabled = !existing;
            observe.Enabled = !ServiceExists();
            if (ServiceExists()) install.Text = "Upgrade";
            if (existing) status.Text = "Existing enrollment will be preserved. No token is needed.";
            install.Click += async delegate {
                busy = true; install.Enabled = false;
                string url = origin.Text.Trim(), secret = token.Text;
                bool readOnly = observe.Checked;
                token.Clear(); status.Text = "Installing and checking Console…";
                try { status.Text = await Task.Run(() => Install(url, secret, readOnly)); }
                catch (Exception error) { status.Text = "Setup did not finish. " + error.Message; }
                finally { secret = null; busy = false; install.Enabled = true; }
            };
            FormClosing += delegate(object sender, FormClosingEventArgs e) { if (busy) e.Cancel = true; };
        }
    }
}
