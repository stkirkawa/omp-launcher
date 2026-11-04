import { invoke } from "@tauri-apps/api/tauri";
import { FormEvent, useState } from "react";

interface AccountInfo {
  id: number;
  login: string;
}

interface AccountAccessProps {
  initialError?: string;
  onAuthenticated: (account: AccountInfo) => void;
}

const LOGIN_PATTERN = /^[A-Za-z]{6,24}$/;

const AccountAccess = ({
  initialError,
  onAuthenticated,
}: AccountAccessProps) => {
  const [mode, setMode] = useState<"login" | "register">("login");
  const [login, setLogin] = useState("");
  const [password, setPassword] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [error, setError] = useState(initialError ?? "");
  const [busy, setBusy] = useState(false);

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (busy) return;

    const normalizedLogin = login.trim();
    if (!LOGIN_PATTERN.test(normalizedLogin)) {
      setError("Логин должен содержать от 6 до 24 английских букв.");
      return;
    }
    if (new TextEncoder().encode(password).length < 8) {
      setError("Пароль должен содержать не менее 8 символов.");
      return;
    }
    if (mode === "register" && password !== confirmation) {
      setError("Пароли не совпадают.");
      return;
    }

    setBusy(true);
    setError("");
    try {
      const command =
        mode === "register" ? "register_account" : "login_account";
      const account = await invoke<AccountInfo>(command, {
        login: normalizedLogin,
        password,
      });
      setPassword("");
      setConfirmation("");
      onAuthenticated(account);
    } catch (requestError) {
      setError(
        requestError instanceof Error
          ? requestError.message
          : String(requestError)
      );
    } finally {
      setBusy(false);
    }
  };

  const switchMode = (nextMode: "login" | "register") => {
    setMode(nextMode);
    setError("");
    setPassword("");
    setConfirmation("");
  };

  return (
    <main className="kirkawa-account-screen">
      <header className="kirkawa-heading">
        <div>
          <div className="kirkawa-eyebrow">KIRKAWA · ACCOUNT</div>
          <h1>
            Kirkawa <span>Launcher</span>
          </h1>
        </div>
        <div className="kirkawa-region">
          <span className="kirkawa-region-dot" />
          ВХОД В АККАУНТ
        </div>
      </header>

      <section className="kirkawa-account-card">
        <div className="kirkawa-account-mark" aria-hidden="true">
          K
        </div>
        <div className="kirkawa-account-copy">
          <div className="kirkawa-server-tag">
            <span className="kirkawa-status-dot online" />
            АККАУНТ KIRKAWA
          </div>
          <h2>{mode === "login" ? "С возвращением." : "Создай аккаунт."}</h2>
          <p>
            {mode === "login"
              ? "Войди, чтобы открыть свой путь в Сан-Андреас."
              : "Логин станет твоим игровым именем на сервере."}
          </p>
        </div>

        <form className="kirkawa-account-form" onSubmit={handleSubmit}>
          <label
            className="kirkawa-account-label"
            htmlFor="kirkawa-account-login"
          >
            ЛОГИН
          </label>
          <div className="kirkawa-account-input">
            <span aria-hidden="true">ID</span>
            <input
              id="kirkawa-account-login"
              autoComplete="username"
              autoCapitalize="none"
              spellCheck={false}
              maxLength={24}
              value={login}
              onChange={(event) => setLogin(event.target.value)}
              placeholder="Только английские буквы"
              disabled={busy}
              required
            />
          </div>
          <small className="kirkawa-account-help">
            От 6 до 24 английских букв. Логин станет игровым ником.
          </small>

          <label
            className="kirkawa-account-label"
            htmlFor="kirkawa-account-password"
          >
            ПАРОЛЬ
          </label>
          <div className="kirkawa-account-input">
            <span aria-hidden="true">••</span>
            <input
              id="kirkawa-account-password"
              type="password"
              autoComplete={
                mode === "register" ? "new-password" : "current-password"
              }
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              placeholder="Не менее 8 символов"
              disabled={busy}
              required
            />
          </div>

          {mode === "register" && (
            <>
              <label
                className="kirkawa-account-label"
                htmlFor="kirkawa-account-confirmation"
              >
                ПОВТОРИ ПАРОЛЬ
              </label>
              <div className="kirkawa-account-input">
                <span aria-hidden="true">••</span>
                <input
                  id="kirkawa-account-confirmation"
                  type="password"
                  autoComplete="new-password"
                  value={confirmation}
                  onChange={(event) => setConfirmation(event.target.value)}
                  placeholder="Ещё раз пароль"
                  disabled={busy}
                  required
                />
              </div>
            </>
          )}

          {error && (
            <p className="kirkawa-account-error" role="alert">
              {error}
            </p>
          )}

          <button
            className="kirkawa-account-submit"
            type="submit"
            disabled={busy}
          >
            {busy
              ? "ПРОВЕРЯЕМ..."
              : mode === "login"
                ? "ВОЙТИ В АККАУНТ"
                : "СОЗДАТЬ АККАУНТ"}
            <span aria-hidden="true">↗</span>
          </button>
        </form>

        <div className="kirkawa-account-switch">
          {mode === "login" ? (
            <>
              <span>Впервые здесь?</span>
              <button type="button" onClick={() => switchMode("register")}>
                СОЗДАТЬ АККАУНТ
              </button>
            </>
          ) : (
            <>
              <span>Уже есть аккаунт?</span>
              <button type="button" onClick={() => switchMode("login")}>
                ВОЙТИ
              </button>
            </>
          )}
        </div>
      </section>
      <footer className="kirkawa-account-footer">
        ВХОД СОХРАНЯЕТСЯ НА ЭТОМ КОМПЬЮТЕРЕ · СЕССИЯ ЗАЩИЩЕНА WINDOWS
      </footer>
    </main>
  );
};

export default AccountAccess;