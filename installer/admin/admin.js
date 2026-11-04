const form = document.querySelector("#release-form");
const versionInput = document.querySelector("#version");
const notesInput = document.querySelector("#changelog");
const archiveInput = document.querySelector("#archive");
const dropzone = document.querySelector("#dropzone");
const fileLabel = document.querySelector("#file-label");
const notesCount = document.querySelector("#notes-count");
const currentVersion = document.querySelector("#current-version");
const publishButton = document.querySelector("#publish-button");
const progressWrap = document.querySelector("#progress-wrap");
const progressLabel = document.querySelector("#progress-label");
const progressPercent = document.querySelector("#progress-percent");
const progressFill = document.querySelector("#progress-fill");
const progressDetail = document.querySelector("#progress-detail");
const errorBox = document.querySelector("#error-box");
const successBox = document.querySelector("#success-box");

const MAX_ARCHIVE_BYTES = 12 * 1024 ** 3;

function formatBytes(value) {
  if (!Number.isFinite(value) || value < 0) return "";
  const units = ["Б", "КБ", "МБ", "ГБ", "ТБ"];
  let size = value;
  let index = 0;
  while (size >= 1024 && index < units.length - 1) {
    size /= 1024;
    index += 1;
  }
  return `${size.toLocaleString("ru-RU", { maximumFractionDigits: 1 })} ${units[index]}`;
}

function showError(message) {
  errorBox.textContent = message;
  errorBox.hidden = false;
  successBox.hidden = true;
}

function setProgress(percent, label, detail) {
  progressWrap.hidden = false;
  progressLabel.textContent = label;
  progressPercent.textContent = `${Math.round(percent)}%`;
  progressFill.style.width = `${Math.max(0, Math.min(100, percent))}%`;
  progressDetail.textContent = detail;
}

function selectArchive(file) {
  if (!file) return;
  const transfer = new DataTransfer();
  transfer.items.add(file);
  archiveInput.files = transfer.files;
  fileLabel.textContent = `${file.name} · ${formatBytes(file.size)}`;
  dropzone.classList.remove("dragover");
  errorBox.hidden = true;
  successBox.hidden = true;
}

function uploadArchive(file, uploadId) {
  return new Promise((resolve, reject) => {
    const request = new XMLHttpRequest();
    request.open("POST", `/api/upload/${encodeURIComponent(uploadId)}`);
    request.setRequestHeader("Content-Type", "application/octet-stream");
    request.upload.addEventListener("progress", (event) => {
      if (!event.lengthComputable) return;
      const percent = (event.loaded / event.total) * 82;
      setProgress(percent, "Загружаем сборку на сервер…", `${formatBytes(event.loaded)} из ${formatBytes(event.total)}`);
    });
    request.upload.addEventListener("load", () => {
      setProgress(84, "Проверяем и подписываем сборку…", "Проверка архива 7-Zip, файлов SAMP и контрольных сумм.");
    });
    request.addEventListener("load", () => {
      let response;
      try {
        response = JSON.parse(request.responseText);
      } catch {
        reject(new Error("Сервер вернул некорректный ответ."));
        return;
      }
      if (request.status < 200 || request.status >= 300) {
        reject(new Error(response.error || `Ошибка сервера: HTTP ${request.status}`));
        return;
      }
      resolve(response);
    });
    request.addEventListener("error", () => reject(new Error("Соединение прервано во время загрузки.")));
    request.addEventListener("abort", () => reject(new Error("Загрузка была отменена.")));
    request.send(file);
  });
}

async function loadCurrentVersion() {
  try {
    const response = await fetch("/api/status", { cache: "no-store" });
    const body = await response.json();
    if (!response.ok) throw new Error(body.error || "Не удалось загрузить текущую версию.");
    currentVersion.textContent = `v${body.version}`;
    versionInput.placeholder = nextPatchVersion(body.version);
  } catch (error) {
    currentVersion.textContent = "Не удалось проверить";
    showError(error instanceof Error ? error.message : String(error));
  }
}

function nextPatchVersion(version) {
  const parts = version.split(".").map(Number);
  if (parts.length < 2 || parts.some((part) => !Number.isSafeInteger(part))) return "";
  while (parts.length < 3) parts.push(0);
  parts[parts.length - 1] += 1;
  return parts.join(".");
}

notesInput.addEventListener("input", () => {
  notesCount.textContent = `${notesInput.value.length.toLocaleString("ru-RU")} / 12000`;
});

archiveInput.addEventListener("change", () => selectArchive(archiveInput.files[0]));

for (const eventName of ["dragenter", "dragover"]) {
  dropzone.addEventListener(eventName, (event) => {
    event.preventDefault();
    dropzone.classList.add("dragover");
  });
}

for (const eventName of ["dragleave", "drop"]) {
  dropzone.addEventListener(eventName, (event) => {
    event.preventDefault();
    dropzone.classList.remove("dragover");
  });
}

dropzone.addEventListener("drop", (event) => {
  const file = event.dataTransfer.files[0];
  if (file) selectArchive(file);
});

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  errorBox.hidden = true;
  successBox.hidden = true;
  progressWrap.hidden = true;

  const file = archiveInput.files[0];
  const version = versionInput.value.trim();
  const changelog = notesInput.value.trim();

  if (!/^\d+(?:\.\d+){1,3}$/.test(version)) {
    showError("Укажи версию из цифр: например, 1.1.0.");
    versionInput.focus();
    return;
  }
  if (!changelog) {
    showError("Напиши список изменений для игроков.");
    notesInput.focus();
    return;
  }
  if (!file) {
    showError("Выбери архив игровой сборки в формате 7z.");
    archiveInput.focus();
    return;
  }
  if (!file.name.toLowerCase().endsWith(".7z") || file.size <= 0 || file.size > MAX_ARCHIVE_BYTES) {
    showError("Загрузи непустой архив 7z размером не больше 12 ГБ.");
    return;
  }

  publishButton.disabled = true;
  versionInput.disabled = true;
  notesInput.disabled = true;
  archiveInput.disabled = true;
  try {
    setProgress(0, "Подготавливаем публикацию…", "Проверяем версию и создаём защищённую загрузку.");
    const prepared = await fetch("/api/prepare", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ version, changelog }),
    });
    const preparedBody = await prepared.json();
    if (!prepared.ok) throw new Error(preparedBody.error || "Сервер не принял данные релиза.");

    const result = await uploadArchive(file, preparedBody.uploadId);
    setProgress(100, "Обновление опубликовано", `Версия ${result.version} · ${result.managedFiles} управляемых файлов · ${formatBytes(result.archiveBytes)}`);
    successBox.textContent = `Версия ${result.version} опубликована. Игроки увидят список изменений и должны будут установить обновление перед входом.`;
    successBox.hidden = false;
    currentVersion.textContent = `v${result.version}`;
    versionInput.value = "";
    notesInput.value = "";
    notesCount.textContent = "0 / 12000";
    archiveInput.value = "";
    fileLabel.textContent = "Перетащи архив сюда или выбери файл";
  } catch (error) {
    showError(error instanceof Error ? error.message : String(error));
    progressWrap.hidden = true;
  } finally {
    publishButton.disabled = false;
    versionInput.disabled = false;
    notesInput.disabled = false;
    archiveInput.disabled = false;
  }
});

void loadCurrentVersion();
