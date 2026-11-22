import { useCallback, useEffect, useId, useRef, useState, type FormEvent } from "react";
import {
  ApiClientError,
  fetchAuthMe,
  getOrCreateDeviceId,
  postAuthLogin,
  postAuthOtpRequest,
  type AuthMeResponse,
  type AuthTokenResponse,
} from "../api/client.ts";
import { attachDialogLightDismissFallback, closeTopLayer } from "../utils/topLayer.ts";

export interface AuthModalProps {
  open: boolean;
  onClose: () => void;
  onAuthenticated: (tokens: AuthTokenResponse, user: AuthMeResponse) => void;
}

type AuthMode = "password" | "otp";

export function AuthModal({ open, onClose, onAuthenticated }: AuthModalProps) {
  const dialogRef = useRef<HTMLDialogElement | null>(null);
  const otpFormRef = useRef<HTMLFormElement | null>(null);
  const otpInputRef = useRef<HTMLInputElement | null>(null);
  const otpAbortRef = useRef<AbortController | null>(null);

  const titleId = useId();
  const phoneErrorId = useId();
  const pwErrorId = useId();
  const otpHintId = useId();
  const otpErrorId = useId();

  const [mode, setMode] = useState<AuthMode>("password");
  const [phoneE164, setPhoneE164] = useState("+51962000100");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);

  const [challengeId, setChallengeId] = useState<string | null>(null);
  const [otpCode, setOtpCode] = useState("");
  const [otpExpiresSec, setOtpExpiresSec] = useState<number | null>(null);

  const [submitting, setSubmitting] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const stopSmsOtpListener = useCallback(() => {
    if (otpAbortRef.current) {
      otpAbortRef.current.abort();
      otpAbortRef.current = null;
    }
  }, []);

  const startSmsOtpListener = useCallback(() => {
    if (typeof window === "undefined" || !("OTPCredential" in window)) {
      return;
    }
    stopSmsOtpListener();
    const ac = new AbortController();
    otpAbortRef.current = ac;

    const getCredentials = navigator.credentials?.get;
    if (typeof getCredentials !== "function") return;

    void getCredentials
      .call(navigator.credentials, {
        otp: { transport: ["sms"] },
        signal: ac.signal,
      } as CredentialRequestOptions)
      .then((cred) => {
        const otpCred = cred as ({ code?: string } | null);
        if (!otpCred?.code) return;
        setOtpCode(otpCred.code);
        if (otpInputRef.current) {
          otpInputRef.current.value = otpCred.code;
        }
        otpFormRef.current?.requestSubmit();
      })
      .catch((err: unknown) => {
        if (err instanceof DOMException && err.name === "AbortError") {
          return;
        }
      });
  }, [stopSmsOtpListener]);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;

    const detachFallback = attachDialogLightDismissFallback(dialog);
    const onDialogClose = () => {
      dialog.returnValue = "";
      stopSmsOtpListener();
      onClose();
    };
    const onCancel = (e: Event) => {
      e.preventDefault();
      void closeTopLayer(dialog);
    };

    dialog.addEventListener("close", onDialogClose);
    dialog.addEventListener("cancel", onCancel);
    return () => {
      detachFallback();
      dialog.removeEventListener("close", onDialogClose);
      dialog.removeEventListener("cancel", onCancel);
    };
  }, [onClose, stopSmsOtpListener]);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open && !dialog.open) {
      setErrorMsg(null);
      dialog.showModal();
    } else if (!open && dialog.open) {
      stopSmsOtpListener();
      void closeTopLayer(dialog);
    }
  }, [open, stopSmsOtpListener]);

  useEffect(() => {
    if (open && mode === "otp" && challengeId) {
      startSmsOtpListener();
      return () => stopSmsOtpListener();
    }
    stopSmsOtpListener();
    return undefined;
  }, [open, mode, challengeId, startSmsOtpListener, stopSmsOtpListener]);

  async function finishLogin(tokens: AuthTokenResponse) {
    const profile = await fetchAuthMe(tokens.access_token);
    onAuthenticated(tokens, profile);
    setPassword("");
    setOtpCode("");
    setChallengeId(null);
    if (dialogRef.current) {
      await closeTopLayer(dialogRef.current);
    }
  }

  async function handlePasswordSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setErrorMsg(null);
    setSubmitting(true);
    try {
      const tokens = await postAuthLogin({
        phone_e164: phoneE164.trim(),
        device_id: getOrCreateDeviceId(),
        password,
      });
      await finishLogin(tokens);
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("No fue posible autenticar con el servidor. Verifique la conexión.");
      }
    } finally {
      setSubmitting(false);
    }
  }

  async function handleRequestOtp(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setErrorMsg(null);
    setSubmitting(true);
    try {
      const resp = await postAuthOtpRequest(phoneE164.trim());
      setChallengeId(resp.challenge_id);
      setOtpExpiresSec(resp.expires_in_sec);
      setOtpCode("");
      startSmsOtpListener();
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("No fue posible solicitar el código OTP SMS.");
      }
    } finally {
      setSubmitting(false);
    }
  }

  async function handleVerifyOtp(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    stopSmsOtpListener();
    if (!challengeId) return;
    setErrorMsg(null);
    setSubmitting(true);
    try {
      const tokens = await postAuthLogin({
        phone_e164: phoneE164.trim(),
        device_id: getOrCreateDeviceId(),
        challenge_id: challengeId,
        otp_code: otpCode.trim(),
      });
      await finishLogin(tokens);
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("Código OTP inválido o expirado.");
      }
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <dialog
      ref={dialogRef}
      className="modal-dialog"
      aria-labelledby={titleId}
      {...({ closedby: "any" } as Record<string, string>)}
    >
      <div className="modal-content">
        <div className="modal-header">
          <h2 id={titleId}>Acceso de Operador Territorial</h2>
          <button
            type="button"
            aria-label="Cerrar diálogo de acceso"
            onClick={() => dialogRef.current && void closeTopLayer(dialogRef.current)}
          >
            Cerrar
          </button>
        </div>

        <div className="auth-mode-switch" role="group" aria-label="Método de autenticación">
          <button
            type="button"
            className={mode === "password" ? "btn btn--primary" : undefined}
            aria-pressed={mode === "password"}
            onClick={() => {
              setErrorMsg(null);
              setMode("password");
            }}
          >
            Contraseña
          </button>
          <button
            type="button"
            className={mode === "otp" ? "btn btn--primary" : undefined}
            aria-pressed={mode === "otp"}
            onClick={() => {
              setErrorMsg(null);
              setMode("otp");
            }}
          >
            SMS OTP (6 dígitos)
          </button>
        </div>

        {errorMsg && (
          <div className="banner banner--danger" role="alert" style={{ marginBlockEnd: "1rem" }}>
            <span>{errorMsg}</span>
          </div>
        )}

        {mode === "password" ? (
          <form onSubmit={handlePasswordSubmit} aria-labelledby={titleId}>
            <div className="field">
              <label htmlFor="auth-phone">Teléfono institucional (E.164)</label>
              <input
                id="auth-phone"
                name="phone_e164"
                type="tel"
                autoComplete="username tel"
                spellCheck={false}
                autoCapitalize="none"
                enterKeyHint="next"
                required
                pattern="^\+?\d{8,19}$"
                value={phoneE164}
                onChange={(e) => setPhoneE164(e.target.value)}
                data-error={phoneErrorId}
              />
              <span id={phoneErrorId} className="error-msg">
                <span role="img" aria-label="Error">
                  ⚠
                </span>{" "}
                Ingrese un teléfono en formato E.164 (ej. +51962000100).
              </span>
            </div>

            <div className="field">
              <label htmlFor="auth-password">Contraseña</label>
              <div className="password-row">
                <input
                  id="auth-password"
                  name="password"
                  type={showPassword ? "text" : "password"}
                  autoComplete="current-password"
                  enterKeyHint="done"
                  required
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  data-error={pwErrorId}
                />
                <button
                  type="button"
                  aria-pressed={showPassword}
                  onClick={() => setShowPassword((v) => !v)}
                >
                  {showPassword ? "Ocultar" : "Mostrar"}
                </button>
              </div>
              <span id={pwErrorId} className="error-msg">
                <span role="img" aria-label="Error">
                  ⚠
                </span>{" "}
                Ingrese su contraseña operativa.
              </span>
            </div>

            <details className="hint" style={{ marginBlockEnd: "1rem" }}>
              <summary>Cuentas de prueba locales (dev seed)</summary>
              <p style={{ marginBlock: "0.5rem 0" }}>
                Tras ejecutar <code>seed_dev_users</code>: <code>+51962000100</code> (COER Admin),{" "}
                <code>+51962000200</code> (Defensa Civil), <code>+51962000300</code> (Salud /
                SAMU).
              </p>
            </details>

            <div className="modal-actions">
              <button
                type="button"
                onClick={() => dialogRef.current && void closeTopLayer(dialogRef.current)}
              >
                Cancelar
              </button>
              <button type="submit" className="btn btn--primary" disabled={submitting}>
                {submitting ? "Verificando..." : "Iniciar sesión"}
              </button>
            </div>
          </form>
        ) : (
          <div>
            <form onSubmit={handleRequestOtp} style={{ marginBlockEnd: "1rem" }}>
              <div className="field">
                <label htmlFor="otp-phone">Teléfono registrado (E.164)</label>
                <input
                  id="otp-phone"
                  name="phone_e164"
                  type="tel"
                  autoComplete="tel"
                  enterKeyHint="next"
                  required
                  pattern="^\+?\d{8,19}$"
                  value={phoneE164}
                  onChange={(e) => setPhoneE164(e.target.value)}
                />
              </div>
              <button type="submit" disabled={submitting}>
                {challengeId ? "Reenviar código SMS" : "Solicitar código SMS"}
              </button>
            </form>

            {challengeId && (
              <form ref={otpFormRef} method="POST" onSubmit={handleVerifyOtp}>
                <div className="field">
                  <label htmlFor="auth-otp-code">Código de verificación OTP</label>
                  <p id={otpHintId} className="hint">
                    Ingrese el código de 6 dígitos enviado por SMS a {phoneE164}
                    {otpExpiresSec ? ` (expira en ${otpExpiresSec} s).` : "."}
                  </p>
                  <input
                    ref={otpInputRef}
                    id="auth-otp-code"
                    name="otp_code"
                    className="otp-input"
                    type="text"
                    inputMode="numeric"
                    autoComplete="one-time-code"
                    enterKeyHint="done"
                    pattern="\d{6}"
                    maxLength={6}
                    aria-describedby={otpHintId}
                    data-error={otpErrorId}
                    required
                    value={otpCode}
                    onChange={(e) => setOtpCode(e.target.value)}
                  />
                  <span id={otpErrorId} className="error-msg">
                    <span role="img" aria-label="Error">
                      ⚠
                    </span>{" "}
                    El código debe tener exactamente 6 dígitos numéricos.
                  </span>
                </div>

                <div className="modal-actions">
                  <button
                    type="button"
                    onClick={() => dialogRef.current && void closeTopLayer(dialogRef.current)}
                  >
                    Cancelar
                  </button>
                  <button type="submit" className="btn btn--primary" disabled={submitting}>
                    {submitting ? "Verificando código..." : "Verificar código"}
                  </button>
                </div>
              </form>
            )}
          </div>
        )}
      </div>
    </dialog>
  );
}
