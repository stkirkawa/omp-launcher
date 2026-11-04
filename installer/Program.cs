using System.Net.Http;
using System.Security.Cryptography;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace KirkawaLauncher;

internal static class Program
{
    [STAThread]
    private static void Main()
    {
        ApplicationConfiguration.Initialize();
        Application.Run(new InstallerForm());
    }
}

internal sealed class InstallerForm : Form
{
    private const string MetadataUrl = "https://api.open.mp/launcher";
    private const string PluginFileName = "omp-client.dll";
    private const string DefaultGamePath = @"B:\samp\GTA San Andreas";
    private static readonly HttpClient Http = new()
    {
        Timeout = TimeSpan.FromMinutes(2)
    };

    private readonly TextBox _gamePath = new();
    private readonly Button _installButton = new();
    private readonly Label _status = new();
    private readonly string _settingsFile = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "KirkawaLauncher",
        "settings.json");

    public InstallerForm()
    {
        Text = "Kirkawa Launcher";
        StartPosition = FormStartPosition.CenterScreen;
        FormBorderStyle = FormBorderStyle.FixedSingle;
        MaximizeBox = false;
        MinimizeBox = true;
        ClientSize = new Size(700, 440);
        BackColor = Color.FromArgb(17, 18, 20);
        ForeColor = Color.FromArgb(244, 241, 237);
        Font = new Font("Segoe UI", 10);

        BuildInterface();
        _gamePath.Text = LoadGamePath();
    }

    private void BuildInterface()
    {
        var header = new Panel
        {
            Dock = DockStyle.Top,
            Height = 112,
            BackColor = Color.FromArgb(25, 26, 28),
            Padding = new Padding(28, 18, 24, 12)
        };
        Controls.Add(header);

        header.Controls.Add(new Label
        {
            Text = "OPEN MULTIPLAYER",
            ForeColor = Color.FromArgb(240, 160, 75),
            Font = new Font("Segoe UI", 9, FontStyle.Bold),
            AutoSize = true,
            Location = new Point(0, 4)
        });
        header.Controls.Add(new Label
        {
            Text = "Kirkawa Launcher",
            ForeColor = Color.FromArgb(246, 243, 238),
            Font = new Font("Segoe UI Semibold", 25, FontStyle.Bold),
            AutoSize = true,
            Location = new Point(-3, 28)
        });
        header.Controls.Add(new Label
        {
            Text = "Установка официального клиента open.mp",
            ForeColor = Color.FromArgb(160, 160, 162),
            Font = new Font("Segoe UI", 10),
            AutoSize = true,
            Location = new Point(1, 76)
        });

        var serverLabel = new Label
        {
            Text = "ВАШ СЕРВЕР",
            ForeColor = Color.FromArgb(137, 138, 141),
            Font = new Font("Segoe UI", 9, FontStyle.Bold),
            AutoSize = true,
            Location = new Point(30, 137)
        };
        Controls.Add(serverLabel);

        var serverCard = new Panel
        {
            Location = new Point(28, 160),
            Size = new Size(644, 73),
            BackColor = Color.FromArgb(27, 28, 30),
            BorderStyle = BorderStyle.FixedSingle
        };
        Controls.Add(serverCard);
        serverCard.Controls.Add(new Label
        {
            Text = "open.mp server",
            ForeColor = Color.FromArgb(245, 242, 237),
            Font = new Font("Segoe UI Semibold", 13, FontStyle.Bold),
            AutoSize = true,
            Location = new Point(16, 12)
        });
        serverCard.Controls.Add(new Label
        {
            Text = "51.75.44.14:2132",
            ForeColor = Color.FromArgb(193, 193, 194),
            Font = new Font("Consolas", 10),
            AutoSize = true,
            Location = new Point(17, 40)
        });

        Controls.Add(new Label
        {
            Text = "ПАПКА УСТАНОВЛЕННОЙ GTA: SAN ANDREAS С SA-MP",
            ForeColor = Color.FromArgb(170, 168, 166),
            Font = new Font("Segoe UI", 9, FontStyle.Bold),
            AutoSize = true,
            Location = new Point(30, 255)
        });

        _gamePath.Location = new Point(29, 280);
        _gamePath.Size = new Size(516, 34);
        _gamePath.BackColor = Color.FromArgb(13, 14, 16);
        _gamePath.ForeColor = Color.FromArgb(245, 242, 237);
        _gamePath.BorderStyle = BorderStyle.FixedSingle;
        _gamePath.Font = new Font("Segoe UI", 10);
        Controls.Add(_gamePath);

        var browseButton = new Button
        {
            Text = "ОБЗОР",
            Location = new Point(555, 279),
            Size = new Size(117, 36),
            FlatStyle = FlatStyle.Flat,
            BackColor = Color.FromArgb(38, 39, 41),
            ForeColor = Color.FromArgb(240, 176, 112),
            Font = new Font("Segoe UI", 9, FontStyle.Bold),
            Cursor = Cursors.Hand
        };
        browseButton.FlatAppearance.BorderColor = Color.FromArgb(88, 72, 55);
        browseButton.Click += BrowseForGameFolder;
        Controls.Add(browseButton);

        _status.Text = "Файлы GTA:SA и SA-MP не изменяются.";
        _status.Location = new Point(31, 331);
        _status.Size = new Size(640, 40);
        _status.ForeColor = Color.FromArgb(147, 148, 150);
        _status.Font = new Font("Segoe UI", 9);
        Controls.Add(_status);

        _installButton.Text = "УСТАНОВИТЬ / ОБНОВИТЬ OPEN.MP";
        _installButton.Location = new Point(29, 378);
        _installButton.Size = new Size(643, 44);
        _installButton.FlatStyle = FlatStyle.Flat;
        _installButton.FlatAppearance.BorderSize = 0;
        _installButton.BackColor = Color.FromArgb(235, 153, 69);
        _installButton.ForeColor = Color.FromArgb(28, 23, 19);
        _installButton.Font = new Font("Segoe UI", 10, FontStyle.Bold);
        _installButton.Cursor = Cursors.Hand;
        _installButton.Click += InstallOrUpdatePlugin;
        Controls.Add(_installButton);
    }

    private string LoadGamePath()
    {
        try
        {
            if (File.Exists(_settingsFile))
            {
                using var document = JsonDocument.Parse(File.ReadAllText(_settingsFile));
                if (document.RootElement.TryGetProperty("gamePath", out var savedPath))
                {
                    var value = savedPath.GetString();
                    if (!string.IsNullOrWhiteSpace(value))
                    {
                        return value;
                    }
                }
            }
        }
        catch (Exception error)
        {
            SetStatus($"Не удалось прочитать настройки: {error.Message}", true);
        }

        return Directory.Exists(DefaultGamePath) ? DefaultGamePath : "";
    }

    private void SaveGamePath(string path)
    {
        var settingsDirectory = Path.GetDirectoryName(_settingsFile)
            ?? throw new InvalidOperationException("Папка настроек не определена.");
        Directory.CreateDirectory(settingsDirectory);
        File.WriteAllText(
            _settingsFile,
            JsonSerializer.Serialize(new { gamePath = path }));
    }

    private void BrowseForGameFolder(object? sender, EventArgs e)
    {
        using var dialog = new FolderBrowserDialog
        {
            Description = "Выберите папку с gta_sa.exe и samp.dll",
            UseDescriptionForTitle = true,
            SelectedPath = Directory.Exists(_gamePath.Text)
                ? _gamePath.Text
                : Environment.GetFolderPath(Environment.SpecialFolder.MyComputer)
        };

        if (dialog.ShowDialog(this) == DialogResult.OK)
        {
            _gamePath.Text = dialog.SelectedPath;
            SetStatus("Папка выбрана. Нажмите кнопку установки.", false);
        }
    }

    private async void InstallOrUpdatePlugin(object? sender, EventArgs e)
    {
        _installButton.Enabled = false;
        UseWaitCursor = true;

        try
        {
            var gamePath = _gamePath.Text.Trim();
            ValidateGameFolder(gamePath);
            SaveGamePath(gamePath);

            SetStatus("Проверяю официальную версию клиента open.mp...", false);
            var plugin = await DownloadVerifiedPlugin();

            var destination = Path.Combine(gamePath, PluginFileName);
            if (File.Exists(destination) &&
                string.Equals(HashFile(destination), plugin.Checksum, StringComparison.OrdinalIgnoreCase))
            {
                SetStatus(
                    $"open.mp уже установлен и проверен (версия {plugin.Version}).",
                    false);
                return;
            }

            var stagingPath = Path.Combine(gamePath, $".{PluginFileName}.kirkawa-new");
            try
            {
                SetStatus("Сохраняю текущий файл и устанавливаю open.mp...", false);

                if (File.Exists(stagingPath))
                {
                    File.Delete(stagingPath);
                }

                await File.WriteAllBytesAsync(stagingPath, plugin.Bytes);
                if (!string.Equals(HashFile(stagingPath), plugin.Checksum, StringComparison.OrdinalIgnoreCase))
                {
                    throw new InvalidDataException("Проверка загруженного файла после записи не прошла.");
                }

                if (File.Exists(destination))
                {
                    var backupDirectory = Path.Combine(
                        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                        "KirkawaLauncher",
                        "backups",
                        DateTime.Now.ToString("yyyyMMdd-HHmmss"));
                    Directory.CreateDirectory(backupDirectory);
                    File.Copy(
                        destination,
                        Path.Combine(backupDirectory, PluginFileName),
                        overwrite: false);
                }

                File.Move(stagingPath, destination, overwrite: true);
                SetStatus(
                    $"Клиент open.mp {plugin.Version} установлен и проверен. SA-MP файлы не изменялись.",
                    false);
            }
            finally
            {
                if (File.Exists(stagingPath))
                {
                    File.Delete(stagingPath);
                }
            }
        }
        catch (Exception error)
        {
            SetStatus(error.Message, true);
            MessageBox.Show(
                this,
                error.Message,
                "Не удалось установить open.mp",
                MessageBoxButtons.OK,
                MessageBoxIcon.Error);
        }
        finally
        {
            UseWaitCursor = false;
            _installButton.Enabled = true;
        }
    }

    private static void ValidateGameFolder(string gamePath)
    {
        if (string.IsNullOrWhiteSpace(gamePath) || !Directory.Exists(gamePath))
        {
            throw new DirectoryNotFoundException("Укажите существующую папку GTA: San Andreas.");
        }

        if (!IsWindowsExecutable(Path.Combine(gamePath, "gta_sa.exe")))
        {
            throw new InvalidDataException("В выбранной папке не найден корректный gta_sa.exe.");
        }

        if (!IsWindowsExecutable(Path.Combine(gamePath, "samp.dll")))
        {
            throw new InvalidDataException(
                "Не найден корректный samp.dll. Выберите папку установленной SA-MP сборки.");
        }
    }

    private static bool IsWindowsExecutable(string path)
    {
        if (!File.Exists(path))
        {
            return false;
        }

        using var stream = File.OpenRead(path);
        if (stream.Length < 64)
        {
            return false;
        }

        Span<byte> header = stackalloc byte[64];
        stream.ReadExactly(header);
        if (header[0] != (byte)'M' || header[1] != (byte)'Z')
        {
            return false;
        }

        var peOffset = BitConverter.ToInt32(header.Slice(0x3c, 4));
        if (peOffset < 64 || peOffset > stream.Length - 6)
        {
            return false;
        }

        stream.Position = peOffset;
        Span<byte> peHeader = stackalloc byte[6];
        stream.ReadExactly(peHeader);

        return peHeader[0] == (byte)'P' &&
               peHeader[1] == (byte)'E' &&
               peHeader[2] == 0 &&
               peHeader[3] == 0 &&
               peHeader[4] == 0x4c &&
               peHeader[5] == 0x01;
    }

    private static async Task<VerifiedPlugin> DownloadVerifiedPlugin()
    {
        using var metadata = await Http.GetStreamAsync(MetadataUrl);
        using var document = await JsonDocument.ParseAsync(metadata);
        var root = document.RootElement;

        string version;
        string downloadUrl;
        string expectedChecksum;

        if (root.TryGetProperty("versions", out var versions) &&
            root.TryGetProperty("version", out var latestVersion))
        {
            version = latestVersion.GetString()
                ?? throw new InvalidDataException("В API отсутствует версия open.mp.");
            if (!versions.TryGetProperty(version, out var versionInfo))
            {
                throw new InvalidDataException($"API не содержит метаданные версии {version}.");
            }

            downloadUrl = versionInfo.GetProperty("ompPluginDownload").GetString()
                ?? throw new InvalidDataException("В API отсутствует ссылка на клиент open.mp.");
            expectedChecksum = versionInfo.GetProperty("ompPluginChecksum").GetString()
                ?? throw new InvalidDataException("В API отсутствует контрольная сумма клиента open.mp.");
        }
        else
        {
            version = root.GetProperty("version").GetString() ?? "unknown";
            downloadUrl = root.GetProperty("ompPluginDownload").GetString()
                ?? throw new InvalidDataException("В API отсутствует ссылка на клиент open.mp.");
            expectedChecksum = root.GetProperty("ompPluginChecksum").GetString()
                ?? throw new InvalidDataException("В API отсутствует контрольная сумма клиента open.mp.");
        }

        if (!Regex.IsMatch(expectedChecksum, "^[a-fA-F0-9]{32}$"))
        {
            throw new InvalidDataException("Контрольная сумма клиента open.mp в API имеет неверный формат.");
        }

        if (!Uri.TryCreate(downloadUrl, UriKind.Absolute, out var uri) ||
            uri.Scheme != Uri.UriSchemeHttps ||
            !string.Equals(uri.Host, "assets.open.mp", StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidDataException("API вернуло недоверенный адрес загрузки клиента open.mp.");
        }

        var bytes = await Http.GetByteArrayAsync(uri);
        if (bytes.Length < 1024 || bytes.Length > 50 * 1024 * 1024)
        {
            throw new InvalidDataException("Размер загруженного клиента open.mp недопустим.");
        }

        var actualChecksum = Convert.ToHexString(MD5.HashData(bytes)).ToLowerInvariant();
        if (!string.Equals(actualChecksum, expectedChecksum, StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidDataException("Контрольная сумма клиента open.mp не совпала; файл не установлен.");
        }

        if (!HasX86PortableExecutableHeader(bytes))
        {
            throw new InvalidDataException("Загруженный файл не является 32-битной Windows DLL.");
        }

        return new VerifiedPlugin(version, expectedChecksum, bytes);
    }

    private static bool HasX86PortableExecutableHeader(byte[] bytes)
    {
        if (bytes.Length < 64 || bytes[0] != (byte)'M' || bytes[1] != (byte)'Z')
        {
            return false;
        }

        var peOffset = BitConverter.ToInt32(bytes, 0x3c);
        return peOffset >= 64 &&
               peOffset <= bytes.Length - 6 &&
               bytes[peOffset] == (byte)'P' &&
               bytes[peOffset + 1] == (byte)'E' &&
               bytes[peOffset + 2] == 0 &&
               bytes[peOffset + 3] == 0 &&
               bytes[peOffset + 4] == 0x4c &&
               bytes[peOffset + 5] == 0x01;
    }

    private static string HashFile(string path)
    {
        using var stream = File.OpenRead(path);
        return Convert.ToHexString(MD5.HashData(stream)).ToLowerInvariant();
    }

    private void SetStatus(string message, bool isError)
    {
        _status.Text = message;
        _status.ForeColor = isError
            ? Color.FromArgb(239, 119, 105)
            : Color.FromArgb(147, 191, 156);
    }

    private sealed record VerifiedPlugin(string Version, string Checksum, byte[] Bytes);
}
