/** Global in-app toasts. Shell hosts the UI; any page can call `showAppToast`. */

export const TOAST_EVENT = "maxspeech-toast";

export type AppToastKind = "success" | "info";

export type AppToastDetail = {
  message: string;
  kind?: AppToastKind;
};

export const ACCOUNT_SAVED_MESSAGE =
  "Changes have officially been saved to your account.";

export function showAppToast(detail: AppToastDetail): void {
  if (typeof window === "undefined") return;
  const message = detail.message?.trim();
  if (!message) return;
  window.dispatchEvent(
    new CustomEvent<AppToastDetail>(TOAST_EVENT, {
      detail: {
        message,
        kind: detail.kind ?? "success",
      },
    }),
  );
}

/** Success toast used after settings land in the user's Supabase account. */
export function showAccountSavedToast(): void {
  showAppToast({ message: ACCOUNT_SAVED_MESSAGE, kind: "success" });
}
