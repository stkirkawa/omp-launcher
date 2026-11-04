import { open } from "@tauri-apps/api/dialog";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/tauri";
import { useCallback, useEffect, useState } from "react";
import { queryServer } from "../../utils/query";
import { startGame } from "../../utils/game";
import AccountAccess from "../AccountAccess";
import { usePersistentServers, useServers } from "../../states/servers";
import { useSettings } from "../../states/settings";
import { useNotification } from "../../states/notification";
import { Server } from "../../utils/types";

const SERVER: Server = {
  ip: "51.75.44.14",
  port: 2132,
  hostname: "open.mp server",
  playerCount: 0,
  maxPlayers: 50,
  gameMode: "Open.MP",
  language: "English",
  hasPassword: false,
  version: "open.mp",
  usingOmp: true,
  partner: false,
  ping: 9999,
  players: [],
  password: "",
  rules: {
    artwork: "",
    mapname: "",
    lagcomp: "",
    version: "",
    weather: "",
    weburl: "",
    worldtime: "",
  },
};

interface UpdateProgress {
  phase: string;
  message: string;
  version: string;
  currentFile: string;
  percent: number;
  downloadedBytes: number;
  totalBytes: number;
  completedFiles: number;
  totalFiles: number;
}

interface ReleaseInfo {
  version: string;
  currentVersion: string | null;
  changelog: string;
  updateAvailable: boolean;
}

interface AccountInfo {
  id: number;
  login: string;
}

const UPDATE_TIPS = [
  "Человек меняется по двум причинам: по первой или второй.",
  "У каждой проблемы есть решение. Но иногда проблема — это ты.",
  "Если долго смотреть в бездну, бездна начнёт спрашивать, когда ты уже зайдёшь на сервер.",
  "Не всё то золото, что блестит. Иногда это просто скин за 500 рублей.",
  "Умный человек найдёт выход. Мудрый не будет заходить.",
  "Если тебе кажется, что ты всё понял, значит, тебе ещё не объяснили.",
  "Каждый человек кузнец своего счастья. Но некоторые даже молоток держат неправильно.",
  "Никогда не сдавайся. Сдавайся только тогда, когда уже сдался.",
  "Опыт приходит с годами. Иногда годы приходят одни.",
  "Успех — это 1% таланта и 99% попыток вспомнить пароль от аккаунта.",
  "Если судьба закрыла дверь, проверь, не вылетел ли ты с сервера.",
  "Не имей сто рублей, а имей сто причин, почему ты опять без денег.",
  "Кто рано встаёт, тот весь день играет.",
  "Тише едешь — дальше будешь. Особенно если у тебя 20 FPS.",
  "Не суди человека по одежде. Судите по тому, как он паркует Sultan.",
  "Слово — не воробей. Вылетит — получишь мут.",
  "Лучше иметь друга в ФБР, чем друга, который знает, где ты живёшь.",
  "Жизнь бьёт ключом. Обычно гаечным.",
  "Не тот богат, у кого много денег, а тот, у кого донат не списался дважды.",
  "Уважение не покупается. Но VIP-статус почему-то покупается.",
  "Если тебя послали — не расстраивайся. Возможно, тебе место тут.",
  "Мужчина должен построить дом, посадить дерево и объяснить админу, почему он не читер.",
  "Если ты упал семь раз, встань восемь. Если не встал — значит, загрузка ещё идёт.",
  "Деньги не главное. Главное — знать, у кого они есть.",
  "Самая короткая дистанция между двумя точками — это когда тебя телепортировал админ.",
  "Не бойся перемен. Бойся, что после обновления слетят настройки.",
  "Если жизнь дала тебе лимоны, продай их на центральном рынке.",
  "Загружаем игру. Ваши проблемы уже в очереди.",
  "Проверяем файлы. В отличие от твоей жизни, тут всё ещё можно исправить.",
  "Подключаемся к серверу. Надеемся, ты хотя бы помнишь пароль.",
  "Ищем смысл жизни… Не найден. Продолжаем загрузку.",
  "Распаковываем архив. Как говорится, терпение и WinRAR.",
  "Синхронизируем данные. Твоя личная жизнь пока не синхронизировалась.",
  "Проверяем обновления. Да, опять. Нет, не спрашивай.",
  "Загружаем текстуры. Чтобы твоя бедность выглядела красиво.",
  "Подготавливаем мир. Мир не готов, но мы его не спрашивали.",
  "Загружаем NPC. Хоть кто-то сегодня выйдет на работу.",
  "Устанавливаем файлы. Мама говорила, что из тебя ничего не выйдет. А лаунчер вот выходит.",
  "Проверяем соединение. Интернет есть, стабильности нет.",
  "Ещё немного… Эту фразу ты слышишь с детства.",
  "Не закрывай лаунчер. Он тоже хочет жить.",
  "Если загрузка зависла, сделай вид, что так задумано.",
  "У каждого своя правда. У античита — своя.",
  "Деньги любят счёт. Мы любим донат.",
  "Всё под контролем. Чьим — неизвестно.",
  "Всё будет хорошо. Но это не точно.",
  "Если загрузка зависла — значит, компьютер задумался о жизни.",
  "Мудрость приходит с опытом. Опыт приходит после бана.",
];

const isDesktopApp = () =>
  typeof window !== "undefined" && "__TAURI_IPC__" in window;

const MainView = () => {
  const [server, setServer] = useState(SERVER);
  const [account, setAccount] = useState<AccountInfo>();
  const [accountChecked, setAccountChecked] = useState(!isDesktopApp());
  const [accountError, setAccountError] = useState<string>();
  const [launching, setLaunching] = useState(false);
  const [updating, setUpdating] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<UpdateProgress>();
  const [releaseInfo, setReleaseInfo] = useState<ReleaseInfo>();
  const [releaseCheckError, setReleaseCheckError] = useState<string>();
  const [checkingRelease, setCheckingRelease] = useState(false);
  const [releaseCheckAttempt, setReleaseCheckAttempt] = useState(0);
  const [updateTip, setUpdateTip] = useState(UPDATE_TIPS[0]);
  const { setFavoritesList } = usePersistentServers();
  const { setServers, setSelected } = useServers();
  const { nickName, setNickName, gtasaPath, setGTASAPath } = useSettings();
  const { showNotification } = useNotification();

  useEffect(() => {
    if (!isDesktopApp()) return;

    let active = true;
    void invoke<AccountInfo | null>("restore_account")
      .then((savedAccount) => {
        if (active && savedAccount) {
          setAccount(savedAccount);
          setNickName(savedAccount.login);
        }
      })
      .catch((error) => {
        if (active) {
          setAccountError(error instanceof Error ? error.message : String(error));
        }
      })
      .finally(() => {
        if (active) setAccountChecked(true);
      });

    return () => {
      active = false;
    };
  }, [setNickName]);

  const handleAuthenticated = useCallback(
    (signedInAccount: AccountInfo) => {
      setAccount(signedInAccount);
      setNickName(signedInAccount.login);
      setAccountError(undefined);
    },
    [setNickName]
  );

  const handleLogout = useCallback(async () => {
    try {
      await invoke("logout_account");
    } catch (error) {
      console.error("Failed to revoke the account session:", error);
      showNotification(
        "Локальный выход выполнен",
        "Не удалось отозвать сессию на сервере. Она автоматически истечёт через 30 дней."
      );
    } finally {
      setAccount(undefined);
      setNickName("");
    }
  }, [setNickName, showNotification]);

  useEffect(() => {
    if (!isDesktopApp()) return;

    let active = true;
    setCheckingRelease(true);
    setReleaseCheckError(undefined);
    void invoke<ReleaseInfo>("check_for_updates")
      .then((info) => {
        if (active) setReleaseInfo(info);
      })
      .catch((error) => {
        if (active) {
          setReleaseCheckError(error instanceof Error ? error.message : String(error));
        }
      })
      .finally(() => {
        if (active) setCheckingRelease(false);
      });

    return () => {
      active = false;
    };
  }, [releaseCheckAttempt]);

  useEffect(() => {
    setFavoritesList([SERVER]);
    setServers([SERVER]);
    setSelected(SERVER);
    queryServer(SERVER, "favorites", "all");

    const refresh = setInterval(() => {
      queryServer(SERVER, "favorites", "all");
    }, 30000);

    const unsubscribe = useServers.subscribe((state) => {
      const updated = state.servers.find(
        (item) => item.ip === SERVER.ip && item.port === SERVER.port
      );
      if (updated) setServer(updated);
    });

    return () => {
      clearInterval(refresh);
      unsubscribe();
    };
  }, [setFavoritesList, setServers, setSelected]);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;

    void listen<UpdateProgress>("kirkawa-update-progress", ({ payload }) => {
      setUpdateProgress(payload);
    })
      .then((stopListening) => {
        if (active) unlisten = stopListening;
        else stopListening();
      })
      .catch((error) => {
        console.error("Failed to listen for game update progress:", error);
      });

    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (!launching) return;

    const showRandomTip = () => {
      setUpdateTip(UPDATE_TIPS[Math.floor(Math.random() * UPDATE_TIPS.length)]);
    };
    showRandomTip();
    const timer = setInterval(showRandomTip, 7000);
    return () => clearInterval(timer);
  }, [launching]);

  const chooseGamePath = useCallback(async () => {
    try {
      const selected = await open({
        defaultPath: gtasaPath.replace(/\//g, "\\") || undefined,
        directory: true,
        multiple: false,
      });

      if (typeof selected === "string") {
        setGTASAPath(selected.replace(/\\/g, "/"));
      }
    } catch (error) {
      console.error("Failed to choose the GTA: San Andreas folder:", error);
      showNotification(
        "Не удалось выбрать папку",
        error instanceof Error ? error.message : String(error)
      );
    }
  }, [gtasaPath, setGTASAPath, showNotification]);

  const handlePlay = useCallback(async () => {
    if (launching) return;

    setUpdateProgress(undefined);
    setLaunching(true);
    const nickname = account?.login ?? nickName.trim();
    setNickName(nickname);

    try {
      await startGame(server, nickname, gtasaPath, "");
    } catch (error) {
      console.error("Failed to start the game:", error);
      showNotification(
        "Не удалось запустить игру",
        error instanceof Error ? error.message : String(error)
      );
    } finally {
      setUpdating(false);
      setLaunching(false);
    }
  }, [account, gtasaPath, launching, nickName, server, setNickName, showNotification]);

  const handleInstallUpdate = useCallback(async () => {
    if (launching) return;
    if (!gtasaPath) {
      showNotification(
        "Не выбрана папка игры",
        "Сначала укажите папку GTA: San Andreas в настройках лаунчера."
      );
      return;
    }

    setUpdateProgress(undefined);
    setLaunching(true);
    setUpdating(true);

    try {
      await invoke("install_or_update", { gamePath: gtasaPath });
      const info = await invoke<ReleaseInfo>("check_for_updates");
      setReleaseInfo(info);
      setReleaseCheckError(undefined);
      showNotification(
        "Обновление установлено",
        `Сборка ${info.version} готова. Теперь можно нажать «Играть».`
      );
    } catch (error) {
      console.error("Failed to install the game update:", error);
      showNotification(
        "Не удалось обновить сборку",
        error instanceof Error ? error.message : String(error)
      );
    } finally {
      setUpdating(false);
      setLaunching(false);
    }
  }, [gtasaPath, launching, showNotification]);

  const isOnline = server.ping < 9999;

  if (isDesktopApp() && !accountChecked) {
    return (
      <main className="kirkawa-account-screen" aria-live="polite">
        <section className="kirkawa-account-card">
          <div className="kirkawa-account-copy">
            <div className="kirkawa-eyebrow">KIRKAWA · ACCOUNT</div>
            <h2>Проверяю аккаунт...</h2>
            <p>Подготавливаем защищённый вход в лаунчер.</p>
          </div>
        </section>
      </main>
    );
  }

  if (isDesktopApp() && !account) {
    return (
      <AccountAccess
        initialError={accountError}
        onAuthenticated={handleAuthenticated}
      />
    );
  }

  return (
    <main className="kirkawa-launcher">
      <header className="kirkawa-heading">
        <div>
          <div className="kirkawa-eyebrow">OPEN MULTIPLAYER</div>
          <h1>
            Kirkawa <span>Launcher</span>
          </h1>
        </div>
        <div className="kirkawa-account-toolbar">
          <div className="kirkawa-region">
            <span className="kirkawa-region-dot" />
            {account?.login ?? "SAN ANDREAS · OPEN.MP"}
          </div>
          {account && (
            <button
              className="kirkawa-account-logout"
              type="button"
              onClick={() => void handleLogout()}
              disabled={launching}
            >
              ВЫЙТИ
            </button>
          )}
        </div>
      </header>

      <section className="kirkawa-hero">
        <div className="kirkawa-hero-glow" />
        <div className="kirkawa-hero-content">
          <div className="kirkawa-server-tag">
            <span
              className={`kirkawa-status-dot ${isOnline ? "online" : ""}`}
            />
            {isOnline ? "СЕРВЕР ДОСТУПЕН" : "СЕРВЕР НЕ В СЕТИ"}
          </div>
          <h2>{server.hostname}</h2>
          <p className="kirkawa-description">
            Твоя история в Сан-Андреасе начинается здесь.
          </p>
          <div className="kirkawa-address">
            <span>АДРЕС СЕРВЕРА</span>
            <strong>
              {server.ip}:{server.port}
            </strong>
          </div>
        </div>
        <div className="kirkawa-hero-mark" aria-hidden="true">
          <span>K</span>
        </div>
        <div className="kirkawa-hero-index">
          01 <i /> 01
        </div>
      </section>

      <section className="kirkawa-controls">
        <div className="kirkawa-field">
          <label htmlFor="kirkawa-nickname">ИГРОВОЙ НИК</label>
          <div className="kirkawa-input-wrap">
            <span className="kirkawa-input-icon">ID</span>
            <input
              id="kirkawa-nickname"
              value={account?.login ?? nickName}
              readOnly={!!account}
              onChange={(event) => setNickName(event.target.value)}
              placeholder="Имя аккаунта"
              maxLength={24}
              autoComplete="off"
            />
          </div>
          <small>Игровой ник — логин твоего аккаунта</small>
        </div>

        <div className="kirkawa-field kirkawa-game-path">
          <label htmlFor="kirkawa-path">ПАПКА GTA: SAN ANDREAS</label>
          <div className="kirkawa-input-wrap">
            <span className="kirkawa-input-icon">↗</span>
            <input
              id="kirkawa-path"
              value={gtasaPath}
              onChange={(event) => setGTASAPath(event.target.value)}
              placeholder="Выберите папку с gta_sa.exe"
              autoComplete="off"
            />
            <button
              className="kirkawa-browse"
              type="button"
              onClick={() => void chooseGamePath()}
            >
              ОБЗОР
            </button>
          </div>
          <small>
            {gtasaPath
              ? "Путь сохранён на этом компьютере"
              : "Нужна установленная GTA: San Andreas"}
          </small>
        </div>
      </section>

      {checkingRelease && (
        <section className="kirkawa-release-check" aria-live="polite">
          ПРОВЕРЯЕМ ВЕРСИЮ ИГРОВОЙ СБОРКИ…
        </section>
      )}

      {releaseCheckError && (
        <section className="kirkawa-release-check kirkawa-release-error" role="alert">
          <div>
            <strong>Не удалось проверить обновления</strong>
            <p>{releaseCheckError}</p>
          </div>
          <button
            type="button"
            onClick={() => setReleaseCheckAttempt((attempt) => attempt + 1)}
          >
            ПОВТОРИТЬ
          </button>
        </section>
      )}

      {releaseInfo?.updateAvailable && !launching && (
        <section className="kirkawa-release-notice" aria-live="polite">
          <div className="kirkawa-release-kicker">ОБЯЗАТЕЛЬНОЕ ОБНОВЛЕНИЕ · {releaseInfo.version}</div>
          <h2>Вышло обновление!</h2>
          <p className="kirkawa-release-required">
            Играть без установки не получится. Сначала обновим сборку.
          </p>
          <div className="kirkawa-release-changelog">
            <strong>Список изменений</strong>
            <p>{releaseInfo.changelog}</p>
          </div>
        </section>
      )}

      <footer className="kirkawa-footer">
        <div className="kirkawa-server-stats">
          <div className="kirkawa-stat">
            <span>ИГРОКИ</span>
            <strong>
              {server.playerCount}
              <i> / {server.maxPlayers || 50}</i>
            </strong>
          </div>
          <div className="kirkawa-stat-divider" />
          <div className="kirkawa-stat">
            <span>ИГРОВОЙ РЕЖИМ</span>
            <strong>{server.gameMode || "Open.MP"}</strong>
          </div>
          <div className="kirkawa-stat-divider" />
          <div className="kirkawa-stat">
            <span>ПИНГ</span>
            <strong>{isOnline ? `${server.ping} ms` : "—"}</strong>
          </div>
        </div>
        {!updating && (
          <button
            className="kirkawa-play"
            type="button"
            onClick={() =>
              void (releaseInfo?.updateAvailable
                ? handleInstallUpdate()
                : handlePlay())
            }
            disabled={
              launching ||
              checkingRelease ||
              (isDesktopApp() && (!releaseInfo || !!releaseCheckError))
            }
          >
            <span>
              {launching
                ? "ЗАПУСК..."
                : releaseInfo?.updateAvailable
                  ? "УСТАНОВИТЬ ОБНОВЛЕНИЕ"
                  : checkingRelease
                    ? "ПРОВЕРКА..."
                    : "ИГРАТЬ"}
            </span>
            <span className="kirkawa-play-arrow">↗</span>
          </button>
        )}
      </footer>
      {launching && updateProgress && (
        <section
          className="kirkawa-update"
          aria-live="polite"
          aria-label="Обновление игровой сборки"
        >
          <div className="kirkawa-update-heading">
            <strong>{updateProgress.message}</strong>
            <span>{updateProgress.percent}%</span>
          </div>
          <div
            className="kirkawa-progress-track"
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={updateProgress.percent}
          >
            <span style={{ width: `${updateProgress.percent}%` }} />
          </div>
          <p>{updateTip}</p>
        </section>
      )}
    </main>
  );
};

export default MainView;
