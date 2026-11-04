#!/usr/bin/env python3
import base64
import hashlib
import json
import os
import re
import shutil
import subprocess
import tempfile
import threading
import time
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path, PurePosixPath
from urllib.parse import urlsplit


HOST = "127.0.0.1"
PORT = 8787
ROOT = Path("/srv/kirkawa/public")
STAGING_ROOT = ROOT / ".staging"
SIGNING_KEY = Path("/etc/kirkawa-updates/manifest-signing.key")
INDEX = Path(__file__).with_name("index.html")
CSS = Path(__file__).with_name("admin.css")
JS = Path(__file__).with_name("admin.js")
MAX_ARCHIVE_BYTES = 12 * 1024**3
MAX_UNPACKED_BYTES = 16 * 1024**3
MAX_ARCHIVE_ENTRIES = 4096
MAX_MANAGED_FILES = 512
MAX_MANAGED_FILE_BYTES = 512 * 1024**2
MAX_MANAGED_TOTAL_BYTES = 2 * 1024**3
MAX_RELEASE_NOTES_CHARS = 12_000
UPLOAD_CHUNK_BYTES = 1024 * 1024
VERSION_PATTERN = re.compile(r"^\d+(?:\.\d+){1,3}$")
MANAGED_PATTERN = re.compile(r"^SAMP/[A-Za-z0-9._/-]+$")
SEVEN_ZIP = shutil.which("7z") or shutil.which("7zz")

sessions = {}
sessions_lock = threading.Lock()
publish_lock = threading.Lock()


class PanelError(Exception):
    pass


def send_signed_json(payload, staging, label):
    data = json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    source = staging / f"{label}.json"
    source.write_bytes(data)
    result = subprocess.run(
        [
            "openssl",
            "pkeyutl",
            "-sign",
            "-rawin",
            "-inkey",
            str(SIGNING_KEY),
            "-in",
            str(source),
        ],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=30,
    )
    return data, base64.b64encode(result.stdout)


def current_manifest():
    path = ROOT / "manifest.json"
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise PanelError("Опубликованный манифест не найден.")
    except (OSError, json.JSONDecodeError) as error:
        raise PanelError(f"Не удалось прочитать текущую версию: {error}")
    version = value.get("version")
    if not isinstance(version, str) or not VERSION_PATTERN.fullmatch(version):
        raise PanelError("Текущий манифест содержит неподдерживаемый формат версии.")
    return value


def version_key(version):
    if not isinstance(version, str) or not VERSION_PATTERN.fullmatch(version):
        raise PanelError("Укажите версию из цифр, например 1.2.0.")
    components = [int(part) for part in version.split(".")]
    if any(component > 9999 for component in components):
        raise PanelError("Каждая часть номера версии должна быть не больше 9999.")
    return tuple(components + [0] * (4 - len(components)))


def validate_metadata(value):
    if not isinstance(value, dict):
        raise PanelError("Переданы неверные данные релиза.")
    version = value.get("version")
    notes = value.get("changelog")
    version_key(version)
    if not isinstance(notes, str):
        raise PanelError("Добавьте список изменений.")
    notes = notes.strip()
    if (
        not notes
        or len(notes) > MAX_RELEASE_NOTES_CHARS
        or any(ord(char) < 32 and char not in "\n\r\t" for char in notes)
    ):
        raise PanelError("Список изменений пуст, слишком длинный или содержит недопустимые символы.")

    published = current_manifest()
    if version_key(version) <= version_key(published["version"]):
        raise PanelError(
            f"Версия {version} должна быть новее опубликованной {published['version']}."
        )
    return {"version": version, "changelog": notes}


def parse_archive_listing(output):
    blocks = []
    block = []
    found_separator = False
    for line in output.splitlines():
        if not found_separator:
            if re.fullmatch(r"-{5,}", line.strip()):
                found_separator = True
            continue
        if not line.strip():
            if block:
                blocks.append(block)
                block = []
        else:
            block.append(line)
    if block:
        blocks.append(block)

    seen = set()
    unpacked_size = 0
    managed = []
    for lines in blocks:
        values = {}
        for line in lines:
            if " = " in line:
                key, value = line.split(" = ", 1)
                values[key] = value
        path = values.get("Path")
        if not path:
            continue
        normalized = path.replace("\\", "/")
        parts = PurePosixPath(normalized).parts
        reserved_names = {
            "CON", "PRN", "AUX", "NUL",
            *(f"COM{index}" for index in range(1, 10)),
            *(f"LPT{index}" for index in range(1, 10)),
        }
        if (
            normalized.startswith("/")
            or not parts
            or any(part in ("", ".", "..") for part in parts)
            or any(
                part.endswith((".", " "))
                or ":" in part
                or any(ord(char) < 32 or char in '<>"|?*' for char in part)
                or part.split(".", 1)[0].upper() in reserved_names
                for part in parts
            )
            or "Symbolic Link" in values
            or "Hard Link" in values
        ):
            raise PanelError(f"В архиве обнаружен небезопасный путь: {path}")

        key = normalized.casefold()
        if key in seen:
            raise PanelError(f"В архиве повторяется путь: {normalized}")
        seen.add(key)
        attributes = values.get("Attributes", "")
        is_directory = "D" in attributes
        try:
            size = int(values.get("Size", "0"))
        except ValueError:
            raise PanelError(f"Не удалось определить размер файла {normalized}.")
        if size < 0 or size > 2 * 1024**3:
            raise PanelError(f"Размер файла в архиве недопустим: {normalized}")
        if is_directory:
            continue

        unpacked_size += size
        if unpacked_size > MAX_UNPACKED_BYTES:
            raise PanelError("Распакованный размер сборки превышает 16 ГБ.")
        if normalized.startswith("SAMP/"):
            if (
                not MANAGED_PATTERN.fullmatch(normalized)
                or any(part in ("", ".", "..") or part.endswith((".", " ")) for part in parts)
            ):
                raise PanelError(f"Недопустимый путь управляемого файла: {normalized}")
            if size > MAX_MANAGED_FILE_BYTES:
                raise PanelError(f"Размер управляемого файла превышает 512 МБ: {normalized}")
            managed.append((normalized, size))

    if not found_separator:
        raise PanelError("Не удалось прочитать список файлов 7-Zip архива.")
    if not seen:
        raise PanelError("Архив не содержит файлов.")
    if len(seen) > MAX_ARCHIVE_ENTRIES:
        raise PanelError("Архив содержит больше 4096 файлов и папок.")
    if not managed or len(managed) > MAX_MANAGED_FILES:
        raise PanelError("В архиве должна находиться папка SAMP с управляемыми файлами.")
    managed_total = sum(size for _, size in managed)
    if managed_total > MAX_MANAGED_TOTAL_BYTES:
        raise PanelError("Общий размер управляемых файлов SAMP превышает 2 ГБ.")
    return managed, unpacked_size


def sha256_file(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while True:
            chunk = source.read(UPLOAD_CHUNK_BYTES)
            if not chunk:
                break
            digest.update(chunk)
    return digest.hexdigest()


def run_seven_zip(arguments, timeout, error_message):
    if not SEVEN_ZIP:
        raise PanelError("На сервере не установлен 7-Zip для проверки игровых архивов.")
    try:
        result = subprocess.run(
            [SEVEN_ZIP, *arguments],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        raise PanelError(error_message)
    except subprocess.CalledProcessError as error:
        details = (error.stderr or error.stdout or b"").decode("utf-8", errors="replace")
        raise PanelError(f"{error_message} {details[-800:].strip()}")
    return result.stdout.decode("utf-8", errors="replace")


def publish_build(metadata, staging, archive):
    if not SIGNING_KEY.is_file():
        raise PanelError("Серверный ключ подписи не найден; публикация остановлена.")
    if not archive.is_file() or archive.stat().st_size == 0:
        raise PanelError("Загруженный архив пуст.")

    run_seven_zip(
        ["t", "-bd", "-bso0", str(archive)],
        timeout=1800,
        error_message="Проверка целостности архива не прошла.",
    )
    listing = run_seven_zip(
        ["l", "-slt", "-bd", str(archive)],
        timeout=180,
        error_message="Не удалось прочитать состав архива.",
    )
    managed, unpacked_size = parse_archive_listing(listing)

    extracted = staging / "extracted"
    extracted.mkdir(mode=0o700)
    run_seven_zip(
        ["x", "-y", "-bd", f"-o{extracted}", str(archive), *(path for path, _ in managed)],
        timeout=900,
        error_message="Не удалось извлечь управляемые файлы SAMP.",
    )

    managed_files = []
    for relative_path, expected_size in managed:
        path = extracted.joinpath(*PurePosixPath(relative_path).parts)
        if path.is_symlink() or not path.is_file():
            raise PanelError(f"Не удалось проверить файл сборки {relative_path}.")
        actual_size = path.stat().st_size
        if actual_size != expected_size:
            raise PanelError(f"Размер {relative_path} не совпадает с архивом.")
        managed_files.append(
            {
                "path": relative_path,
                "size": actual_size,
                "sha256": sha256_file(path),
            }
        )

    archive_size = archive.stat().st_size
    manifest = {
        "version": metadata["version"],
        "gamePackage": {
            "size": archive_size,
            "unpackedSize": unpacked_size,
            "sha256": sha256_file(archive),
        },
        "files": managed_files,
    }
    release_notes = {
        "version": metadata["version"],
        "changelog": metadata["changelog"],
    }
    manifest_bytes, manifest_signature = send_signed_json(manifest, staging, "manifest")
    release_bytes, release_signature = send_signed_json(release_notes, staging, "release")
    (staging / "manifest.sig").write_bytes(manifest_signature)
    publish_files(
        metadata,
        staging,
        archive,
        managed,
        manifest_bytes,
        manifest_signature,
        release_bytes,
        release_signature,
    )
    return {
        "version": metadata["version"],
        "managedFiles": len(managed_files),
        "archiveBytes": archive_size,
        "unpackedBytes": unpacked_size,
        "sha256": manifest["gamePackage"]["sha256"],
    }


def publish_files(metadata, staging, archive, managed, manifest, manifest_signature,
                  release_notes, release_signature):
    version = metadata["version"]
    release_directory = ROOT / "releases" / version
    if release_directory.exists():
        raise PanelError(f"Релиз {version} уже подготовлен; укажите новый номер версии.")

    release_directory.parent.mkdir(parents=True, exist_ok=True)
    os.chmod(release_directory.parent, 0o755)
    release_directory.mkdir(mode=0o755)
    os.chmod(release_directory, 0o755)
    notes_stage = staging / "release.json"
    signature_stage = staging / "release.sig"
    notes_stage.write_bytes(release_notes)
    signature_stage.write_bytes(release_signature)
    os.chmod(notes_stage, 0o644)
    os.chmod(signature_stage, 0o644)

    files_to_install = [(archive, ROOT / "Kirkawa-Game.7z")]
    for relative_path, _ in managed:
        source = staging / "extracted" / Path(*PurePosixPath(relative_path).parts)
        files_to_install.append((source, ROOT / "files" / Path(*PurePosixPath(relative_path).parts)))
    files_to_install.extend(
        [
            (notes_stage, release_directory / "release.json"),
            (signature_stage, release_directory / "release.sig"),
            (staging / "manifest.sig", ROOT / "manifest.sig"),
            (staging / "manifest.json", ROOT / "manifest.json"),
        ]
    )

    staged_targets = []
    backup_targets = []
    installed_targets = []
    token = uuid.uuid4().hex
    try:
        for source, target in files_to_install:
            target.parent.mkdir(parents=True, exist_ok=True)
            os.chmod(target.parent, 0o755)
            staged_target = target.with_name(f".{target.name}.{token}.upload")
            os.replace(source, staged_target)
            os.chmod(staged_target, 0o644)
            staged_targets.append((staged_target, target))

        for source, target in staged_targets:
            if target.exists():
                backup = staging / f"backup-{len(backup_targets)}"
                os.replace(target, backup)
                backup_targets.append((backup, target))
            os.replace(source, target)
            installed_targets.append(target)

    except Exception:
        for target in reversed(installed_targets):
            try:
                target.unlink()
            except FileNotFoundError:
                pass
        for backup, target in reversed(backup_targets):
            if backup.exists():
                os.replace(backup, target)
        try:
            shutil.rmtree(release_directory)
        except FileNotFoundError:
            pass
        raise


class ReleaseHandler(BaseHTTPRequestHandler):
    server_version = "KirkawaReleasePanel/1.0"

    def log_message(self, format_string, *args):
        print(f"{self.log_date_time_string()} {self.client_address[0]} {format_string % args}")

    def _send(self, status, body, content_type, headers=None):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("X-Frame-Options", "DENY")
        self.send_header("Referrer-Policy", "no-referrer")
        self.send_header("Content-Security-Policy",
                         "default-src 'self'; script-src 'self'; style-src 'self'; "
                         "connect-src 'self'; img-src 'self' data:; base-uri 'none'; "
                         "form-action 'self'; frame-ancestors 'none'; object-src 'none'")
        for name, value in (headers or {}).items():
            self.send_header(name, value)
        self.end_headers()
        self.wfile.write(body)

    def _json(self, status, value):
        body = json.dumps(value, ensure_ascii=False).encode("utf-8")
        self._send(status, body, "application/json; charset=utf-8")

    def _read_json(self, maximum_bytes):
        try:
            length = int(self.headers.get("Content-Length", "-1"))
        except ValueError:
            raise PanelError("Некорректный размер запроса.")
        if length < 0 or length > maximum_bytes:
            raise PanelError("Размер данных формы превышает допустимый предел.")
        try:
            value = json.loads(self.rfile.read(length).decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError):
            raise PanelError("Не удалось прочитать данные формы.")
        return value

    def _check_local_request(self):
        if self.client_address[0] not in ("127.0.0.1", "::1"):
            raise PanelError("Панель доступна только через локальный SSH-туннель.")
        host = self.headers.get("Host", "")
        if not re.fullmatch(r"(?:127\.0\.0\.1|localhost):\d{1,5}", host):
            raise PanelError("Недопустимый адрес панели.")
        hostname, port_text = host.rsplit(":", 1)
        port = int(port_text)
        if port < 1024 or port > 65535:
            raise PanelError("Недопустимый адрес панели.")
        origin = self.headers.get("Origin")
        if origin and origin != f"http://{hostname}:{port}":
            raise PanelError("Недопустимый источник запроса.")

    def do_GET(self):
        try:
            self._check_local_request()
            path = urlsplit(self.path).path
            if path == "/":
                self._send(200, INDEX.read_bytes(), "text/html; charset=utf-8")
            elif path == "/assets/admin.css":
                self._send(200, CSS.read_bytes(), "text/css; charset=utf-8")
            elif path == "/assets/admin.js":
                self._send(200, JS.read_bytes(), "text/javascript; charset=utf-8")
            elif path == "/api/status":
                manifest = current_manifest()
                self._json(200, {"version": manifest["version"], "ready": True})
            else:
                self._json(404, {"error": "Страница не найдена."})
        except PanelError as error:
            self._json(400, {"error": str(error)})
        except OSError as error:
            self._json(500, {"error": f"Не удалось прочитать данные панели: {error}"})

    def do_POST(self):
        try:
            self._check_local_request()
            path = urlsplit(self.path).path
            if path == "/api/prepare":
                self._prepare_upload()
            elif path.startswith("/api/upload/"):
                self._receive_upload(path.rsplit("/", 1)[-1])
            else:
                self._json(404, {"error": "Адрес загрузки не найден."})
        except PanelError as error:
            self._json(400, {"error": str(error)})
        except Exception as error:
            print(f"Release panel error: {error!r}")
            self._json(500, {"error": "Не удалось опубликовать сборку. Проверьте журнал сервера."})

    def _prepare_upload(self):
        metadata = validate_metadata(self._read_json(32 * 1024))
        token = uuid.uuid4().hex
        staging = Path(tempfile.mkdtemp(prefix=f"{token}-", dir=STAGING_ROOT))
        with sessions_lock:
            now = time.monotonic()
            for old_token, session in list(sessions.items()):
                if now - session["created"] > 7200:
                    shutil.rmtree(session["staging"], ignore_errors=True)
                    del sessions[old_token]
            sessions[token] = {"metadata": metadata, "staging": staging, "created": now}
        self._json(200, {"uploadId": token})

    def _receive_upload(self, token):
        if not re.fullmatch(r"[a-f0-9]{32}", token):
            raise PanelError("Неверный идентификатор загрузки.")
        with sessions_lock:
            session = sessions.pop(token, None)
        if not session:
            raise PanelError("Загрузка устарела. Обновите страницу и попробуйте снова.")
        staging = session["staging"]
        archive = staging / "Kirkawa-Game.7z"
        try:
            try:
                length = int(self.headers.get("Content-Length", "-1"))
            except ValueError:
                raise PanelError("Некорректный размер архива.")
            if length <= 0 or length > MAX_ARCHIVE_BYTES:
                raise PanelError("Размер архива должен быть от 1 байта до 12 ГБ.")
            if self.headers.get("Content-Type") != "application/octet-stream":
                raise PanelError("Загрузите архив игры в формате 7z.")

            remaining = length
            with archive.open("xb") as target:
                while remaining:
                    chunk = self.rfile.read(min(UPLOAD_CHUNK_BYTES, remaining))
                    if not chunk:
                        raise PanelError("Передача архива оборвалась до завершения.")
                    target.write(chunk)
                    remaining -= len(chunk)
                target.flush()
                os.fsync(target.fileno())
            with archive.open("rb") as uploaded:
                archive_signature = uploaded.read(6)
            if archive_signature != b"7z\xbc\xaf'\x1c":
                raise PanelError("Выбранный файл не является архивом 7z.")

            with publish_lock:
                result = publish_build(session["metadata"], staging, archive)
            self._json(200, {"published": True, **result})
        finally:
            shutil.rmtree(staging, ignore_errors=True)


def main():
    if not STAGING_ROOT.is_dir() or STAGING_ROOT.is_symlink():
        raise RuntimeError(f"Недоступна закрытая папка загрузки: {STAGING_ROOT}")
    if not SEVEN_ZIP:
        raise RuntimeError("Не установлен 7-Zip. Установите пакет p7zip-full.")
    if not SIGNING_KEY.is_file():
        raise RuntimeError(f"Ключ подписи не найден: {SIGNING_KEY}")
    server = ThreadingHTTPServer((HOST, PORT), ReleaseHandler)
    print(f"Kirkawa release panel listening on http://{HOST}:{PORT}")
    server.serve_forever()


if __name__ == "__main__":
    main()
