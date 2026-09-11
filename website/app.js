(() => {
  const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // —— Hero flow stage (curved ribbon + liquid-glass pill) ——
  const FLOW_SENTENCE =
    "hold a hotkey speak naturally MaxSpeech types into Gmail Slack Cursor Notion cleaned ready to send · ";
  const FLOW_ACTIVE = [
    "hold a hotkey · speak naturally",
    "types into Gmail · Slack · Cursor",
    "grammar cleaned · ready to send",
    "speak · it types anywhere",
  ];
  const FLOW_TOASTS = [
    "Fixed grammar",
    "Cleaned up",
    "Removed fillers",
    "Ready to send",
  ];

  const OVERLAY_BARS = 20;

  function fillOverlayWaves() {
    document.querySelectorAll(".ms-overlay-wave").forEach((root) => {
      if (root.childElementCount) return;
      for (let i = 0; i < OVERLAY_BARS; i++) {
        const el = document.createElement("i");
        el.style.setProperty("--i", String(i));
        root.appendChild(el);
      }
    });
  }
  fillOverlayWaves();

  const stage = document.getElementById("hero-flow-stage");
  const trailLeft = document.getElementById("flow-trail-left");
  const trailRight = document.getElementById("flow-trail-right");
  const flowActive = document.getElementById("flow-active");
  const flowToast = document.getElementById("flow-toast");
  const flowToastTitle = document.getElementById("flow-toast-title");

  if (trailLeft) trailLeft.textContent = FLOW_SENTENCE.repeat(4);
  if (trailRight) trailRight.textContent = FLOW_SENTENCE.repeat(4);
  if (flowActive) flowActive.textContent = FLOW_ACTIVE[0];
  if (flowToastTitle) flowToastTitle.textContent = FLOW_TOASTS[0];

  let flowVisible = true;
  let phraseIndex = 0;
  let leftOffset = 0;
  let rightOffset = 18;
  let lastPhrase = performance.now();
  let lastWave = 0;

  function paintOverlayWaves(now) {
    const t = now / 1000;
    document.querySelectorAll(".ms-overlay-wave").forEach((root) => {
      const active = root.getAttribute("data-active") !== "false";
      const bars = root.querySelectorAll("i");
      const n = bars.length;
      if (!n) return;
      bars.forEach((bar, i) => {
        const mid =
          1 - (Math.abs(i - (n - 1) / 2) / Math.max(1, (n - 1) / 2)) * 0.18;
        const live = active ? 1 : 0.42;
        const wave =
          (active ? 0.35 : 0.2) +
          live *
            (0.45 * Math.sin(t * 5.2 + i * 0.55) +
              0.2 * Math.sin(t * 9.1 + i * 1.1));
        bar.style.height = `${Math.max(3, Math.round(wave * mid * 18))}px`;
      });
    });
  }

  if (stage && "IntersectionObserver" in window) {
    const io = new IntersectionObserver(
      ([entry]) => {
        flowVisible = entry.isIntersecting;
      },
      { rootMargin: "80px", threshold: 0.05 },
    );
    io.observe(stage);
  }

  function tickFlow(now) {
    if (!reduceMotion) {
      if (flowVisible) {
        leftOffset = (leftOffset + 0.004) % 100;
        rightOffset = (rightOffset + 0.0035) % 100;
        trailLeft?.setAttribute("startOffset", `${-leftOffset}%`);
        trailRight?.setAttribute("startOffset", `${-rightOffset}%`);

        if (now - lastPhrase > 4200) {
          phraseIndex = (phraseIndex + 1) % FLOW_ACTIVE.length;
          if (flowActive) flowActive.textContent = FLOW_ACTIVE[phraseIndex];
          if (flowToastTitle) flowToastTitle.textContent = FLOW_TOASTS[phraseIndex];
          if (flowToast) {
            flowToast.classList.remove("is-pop");
            void flowToast.offsetWidth;
            flowToast.classList.add("is-pop");
          }
          lastPhrase = now;
        }
      }

      if (now - lastWave > 33) {
        paintOverlayWaves(now);
        lastWave = now;
      }
    } else {
      paintOverlayWaves(0);
    }
    requestAnimationFrame(tickFlow);
  }

  requestAnimationFrame(tickFlow);

  // —— Auto-detect download CTA ——
  (function setupDownloadCta() {
    const primary = document.getElementById("cta-primary");
    const label = document.getElementById("cta-label");
    const icon = document.getElementById("cta-icon");
    const auto = document.getElementById("cta-auto");
    const meta = document.getElementById("cta-meta");
    const platforms = document.getElementById("cta-platforms");
    if (!primary || !label || !icon) return;

    const WIN =
      "/downloads/MaxSpeech_x64-setup.exe";
    const ICONS = {
      windows:
        '<svg class="os-svg os-win" viewBox="0 0 24 24" width="18" height="18"><path fill="currentColor" d="M3 5.2 10.6 4.1v7.1H3zm8.4-1.3L21 2.5v8.7h-9.6zM3 12.8h7.6v7.1L3 18.8zm8.4 0H21v8.7l-9.6-1.4z"/></svg>',
      mac:
        '<svg class="os-svg os-mac" viewBox="0 0 24 24" width="18" height="18"><path fill="currentColor" d="M16.4 13.1c0-2.3 1.9-3.4 2-3.5-1.1-1.6-2.8-1.8-3.4-1.8-1.4-.2-2.8.9-3.5.9-.7 0-1.9-.8-3.1-.8-1.6 0-3.1 1-3.9 2.4-1.7 2.9-.4 7.2 1.2 9.6.8 1.1 1.7 2.4 3 2.4 1.2 0 1.6-.8 3-.8s1.7.8 3 .8c1.2 0 2-1.1 2.8-2.2.9-1.3 1.2-2.5 1.3-2.6-.03-.01-2.4-1-2.4-4.4zM14.3 5.7c.7-.8 1.1-1.9 1-3-.9.1-2.1.6-2.8 1.4-.6.7-1.2 1.8-1 2.9 1 .1 2.1-.5 2.8-1.3z"/></svg>',
      linux:
        '<svg class="os-svg os-linux" viewBox="0 0 24 24" width="18" height="18"><path fill="currentColor" d="M12.504 0c-.155 0-.315.008-.48.021-4.226.333-3.105 4.807-3.17 6.298-.076 1.092-.3 1.953-1.05 3.02-.885 1.051-2.127 2.75-2.716 4.521-.278.832-.41 1.684-.287 2.489a.424.424 0 00-.11.135c-.26.268-.45.6-.663.839-.199.199-.485.267-.797.4-.313.136-.658.269-.864.68-.09.189-.136.394-.132.602 0 .199.027.4.055.536.058.399.116.728.04.97-.249.68-.28 1.145-.106 1.484.174.334.535.47.94.601.81.2 1.91.135 2.774.6.926.466 1.866.67 2.616.47.526-.116.97-.464 1.208-.946.587-.003 1.23-.269 2.26-.334.699-.058 1.574.267 2.577.2.025.134.063.198.114.333l.003.003c.391.778 1.113 1.132 1.884 1.071.771-.06 1.592-.536 2.257-1.306.631-.765 1.683-1.084 2.378-1.503.348-.199.629-.469.649-.853.023-.4-.2-.811-.714-1.376v-.097l-.003-.003c-.17-.2-.25-.535-.338-.926-.085-.401-.182-.786-.492-1.046h-.003c-.059-.054-.123-.067-.188-.135a.357.357 0 00-.19-.064c.431-1.278.264-2.55-.173-3.694-.533-1.41-1.465-2.638-2.175-3.483-.796-1.005-1.576-1.957-1.56-3.368.026-2.152.236-6.133-3.544-6.139zm.529 3.405h.013c.213 0 .396.062.584.198.19.135.33.332.438.533.105.259.158.459.166.724 0-.02.006-.04.006-.06v.105a.086.086 0 01-.004-.021l-.004-.024a1.807 1.807 0 01-.15.706.953.953 0 01-.213.335.71.71 0 00-.088-.042c-.104-.045-.198-.064-.284-.133a1.312 1.312 0 00-.22-.066c.05-.06.146-.133.183-.198.053-.128.082-.264.088-.402v-.02a1.21 1.21 0 00-.061-.4c-.045-.134-.101-.2-.183-.333-.084-.066-.167-.132-.267-.132h-.016c-.093 0-.176.03-.262.132a.8.8 0 00-.205.334 1.18 1.18 0 00-.09.4v.019c.002.089.008.179.02.267-.193-.067-.438-.135-.607-.202a1.635 1.635 0 01-.018-.2v-.02a1.772 1.772 0 01.15-.768c.082-.22.232-.406.43-.533a.985.985 0 01.594-.2zm-2.962.059h.036c.142 0 .27.048.399.135.146.129.264.288.344.465.09.199.14.4.153.667v.004c.007.134.006.2-.002.266v.08c-.03.007-.056.018-.083.024-.152.055-.274.135-.393.2.012-.09.013-.18.003-.267v-.015c-.012-.133-.04-.2-.082-.333a.613.613 0 00-.166-.267.248.248 0 00-.183-.064h-.021c-.071.006-.13.04-.186.132a.552.552 0 00-.12.27.944.944 0 00-.023.33v.015c.012.135.037.2.08.334.046.134.098.2.166.268.01.009.02.018.034.024-.07.057-.117.07-.176.136a.304.304 0 01-.131.068 2.62 2.62 0 01-.275-.402 1.772 1.772 0 01-.155-.667 1.759 1.759 0 01.08-.668 1.43 1.43 0 01.283-.535c.128-.133.26-.2.418-.2zm1.37 1.706c.332 0 .733.065 1.216.399.293.2.523.269 1.052.468h.003c.255.136.405.266.478.399v-.131a.571.571 0 01.016.47c-.123.31-.516.643-1.063.842v.002c-.268.135-.501.333-.775.465-.276.135-.588.292-1.012.267a1.139 1.139 0 01-.448-.067 3.566 3.566 0 01-.322-.198c-.195-.135-.363-.332-.612-.465v-.005h-.005c-.4-.246-.616-.512-.686-.71-.07-.268-.005-.47.193-.6.224-.135.38-.271.483-.336.104-.074.143-.102.176-.131h.002v-.003c.169-.202.436-.47.839-.601.139-.036.294-.065.466-.065zm2.8 2.142c.358 1.417 1.196 3.475 1.735 4.473.286.534.855 1.659 1.102 3.024.156-.005.33.018.513.064.646-1.671-.546-3.467-1.089-3.966-.22-.2-.232-.335-.123-.335.59.534 1.365 1.572 1.646 2.757.13.535.16 1.104.021 1.67.067.028.135.06.205.067 1.032.534 1.413.938 1.23 1.537v-.043c-.06-.003-.12 0-.18 0h-.016c.151-.467-.182-.825-1.065-1.224-.915-.4-1.646-.336-1.77.465-.008.043-.013.066-.018.135-.068.023-.139.053-.209.064-.43.268-.662.669-.793 1.187-.13.533-.17 1.156-.205 1.869v.003c-.02.334-.17.838-.319 1.35-1.5 1.072-3.58 1.538-5.348.334a2.645 2.645 0 00-.402-.533 1.45 1.45 0 00-.275-.333c.182 0 .338-.03.465-.067a.615.615 0 00.314-.334c.108-.267 0-.697-.345-1.163-.345-.467-.931-.995-1.788-1.521-.63-.4-.986-.87-1.15-1.396-.165-.534-.143-1.085-.015-1.645.245-1.07.873-2.11 1.274-2.763.107-.065.037.135-.408.974-.396.751-1.14 2.497-.122 3.854a8.123 8.123 0 01.647-2.876c.564-1.278 1.743-3.504 1.836-5.268.048.036.217.135.289.202.218.133.38.333.59.465.21.201.477.335.876.335.039.003.075.006.11.006.412 0 .73-.134.997-.268.29-.134.52-.334.74-.4h.005c.467-.135.835-.402 1.044-.7zm2.185 8.958c.037.6.343 1.245.882 1.377.588.134 1.434-.333 1.791-.765l.211-.01c.315-.007.577.01.847.268l.003.003c.208.199.305.53.391.876.085.4.154.78.409 1.066.486.527.645.906.636 1.14l.003-.007v.018l-.003-.012c-.015.262-.185.396-.498.595-.63.401-1.746.712-2.457 1.57-.618.737-1.37 1.14-2.036 1.191-.664.053-1.237-.2-1.574-.898l-.005-.003c-.21-.4-.12-1.025.056-1.69.176-.668.428-1.344.463-1.897.037-.714.076-1.335.195-1.814.12-.465.308-.797.641-.984l.045-.022zm-10.814.049h.01c.053 0 .105.005.157.014.376.055.706.333 1.023.752l.91 1.664.003.003c.243.533.754 1.064 1.189 1.637.434.598.77 1.131.729 1.57v.006c-.057.744-.48 1.148-1.125 1.294-.645.135-1.52.002-2.395-.464-.968-.536-2.118-.469-2.857-.602-.369-.066-.61-.2-.723-.4-.11-.2-.113-.602.123-1.23v-.004l.002-.003c.117-.334.03-.752-.027-1.118-.055-.401-.083-.71.043-.94.16-.334.396-.4.69-.533.294-.135.64-.202.915-.47h.002v-.002c.256-.268.445-.601.668-.838.19-.201.38-.336.663-.336zm7.159-9.074c-.435.201-.945.535-1.488.535-.542 0-.97-.267-1.28-.466-.154-.134-.28-.268-.373-.335-.164-.134-.144-.333-.074-.333.109.016.129.134.199.2.096.066.215.2.36.333.292.2.68.467 1.167.467.485 0 1.053-.267 1.398-.466.195-.135.445-.334.648-.467.156-.136.149-.267.279-.267.128.016.034.134-.147.332a8.097 8.097 0 01-.69.468zm-1.082-1.583V5.64c-.006-.02.013-.042.029-.05.074-.043.18-.027.26.004.063 0 .16.067.15.135-.006.049-.085.066-.135.066-.055 0-.092-.043-.141-.068-.052-.018-.146-.008-.163-.065zm-.551 0c-.02.058-.113.049-.166.066-.047.025-.086.068-.14.068-.05 0-.13-.02-.136-.068-.01-.066.088-.133.15-.133.08-.031.184-.047.259-.005.019.009.036.03.03.05v.02h.003z"/></svg>',
      android:
        '<svg class="os-svg os-android" viewBox="0 0 24 24" width="18" height="18"><path fill="currentColor" d="M17.6 9.48 19.2 6.7a.75.75 0 0 0-1.3-.75l-1.55 2.68A8.1 8.1 0 0 0 12 7.5a8.1 8.1 0 0 0-4.35 1.13L6.1 5.95a.75.75 0 1 0-1.3.75l1.6 2.78A7.7 7.7 0 0 0 4 14.25v.5h16v-.5a7.7 7.7 0 0 0-2.4-4.77ZM7.75 13a.9.9 0 1 1 0-1.8.9.9 0 0 1 0 1.8Zm8.5 0a.9.9 0 1 1 0-1.8.9.9 0 0 1 0 1.8ZM6.2 16.5c.4 2.5 2.7 4.5 5.8 4.5s5.4-2 5.8-4.5H6.2Z"/></svg>',
    };

    const p = (navigator.platform || "").toLowerCase();
    const ua = (navigator.userAgent || "").toLowerCase();
    let os = "windows";
    if (ua.includes("android")) os = "android";
    else if (/iphone|ipad|ipod/.test(ua)) os = "ios";
    else if (p.includes("mac") || ua.includes("mac")) os = "mac";
    else if (p.includes("linux") || ua.includes("linux")) os = "linux";

    icon.innerHTML = ICONS[os] || ICONS.windows;
    if (os === "mac") {
      primary.href = "/mac";
      primary.removeAttribute("download");
      label.textContent = "Download for Mac";
      if (auto) auto.textContent = "Auto-select · Mac detected";
      if (meta) {
        meta.innerHTML =
          'Full Mac installer · <a href="/mac">install help</a>';
      }
    } else if (os === "linux") {
      primary.href = "/linux";
      primary.removeAttribute("download");
      label.textContent = "Download for Linux";
      if (auto) auto.textContent = "Auto-select · Linux detected";
      if (meta) {
        meta.innerHTML =
          'Full Linux package · <a href="/linux">install help</a>';
      }
    } else if (os === "android" || os === "ios") {
      primary.href = "/android";
      primary.removeAttribute("download");
      icon.innerHTML = ICONS.android;
      label.textContent = "Get MaxSpeech for Android";
      if (auto) {
        auto.textContent =
          os === "ios"
            ? "iPhone detected · phone app is Android"
            : "Auto-select · Android detected";
      }
      if (meta) {
        meta.innerHTML =
          os === "ios"
            ? 'Need desktop? Pick Windows, Mac, or Linux below · <a href="/android">Android install</a>'
            : 'APK sideload · about 2 MB · <a href="/android">install help</a>';
      }
    } else {
      primary.href = WIN;
      primary.setAttribute("download", "MaxSpeech_x64-setup.exe");
      label.textContent = "Download for Windows";
      if (auto) auto.textContent = "Auto-select · Windows detected";
      if (meta) {
        meta.innerHTML =
          'Full Windows installer · sets everything up for you · <a href="/windows">install help</a>';
      }
    }

    if (platforms) {
      const activeOs = os === "ios" ? "android" : os;
      platforms.querySelectorAll("[data-os]").forEach((el) => {
        el.classList.toggle("is-active", el.getAttribute("data-os") === activeOs);
      });
    }
  })();

  // —— Playground STT ——
  const micBtn = document.getElementById("mic-btn");
  const micLabel = document.getElementById("mic-label");
  const micWave = document.getElementById("mic-wave");
  const micHint = document.getElementById("mic-hint");
  const clearBtn = document.getElementById("clear-btn");
  const previewText = document.getElementById("preview-text");
  const previewPlaceholder = document.getElementById("preview-placeholder");
  const previewTitle = document.getElementById("preview-title");

  const SpeechRecognition =
    window.SpeechRecognition || window.webkitSpeechRecognition;
  let recognition = null;
  let listening = false;
  let finalTranscript = "";

  function setPreview(text, interim = "") {
    const shown = (text + interim).trim();
    if (!shown) {
      previewText.hidden = true;
      previewPlaceholder.hidden = false;
      previewText.textContent = "";
      return;
    }
    previewPlaceholder.hidden = true;
    previewText.hidden = false;
    previewText.innerHTML = "";
    previewText.appendChild(document.createTextNode(shown));
    const caret = document.createElement("span");
    caret.className = "caret";
    previewText.appendChild(caret);
  }

  function setListeningUI(on) {
    listening = on;
    micBtn?.setAttribute("aria-pressed", on ? "true" : "false");
    if (micLabel) micLabel.textContent = on ? "Listening" : "Hold to talk";
    if (micWave) micWave.dataset.active = on ? "true" : "false";
  }

  function stopListening() {
    if (!listening) return;
    setListeningUI(false);
    try {
      recognition?.stop();
    } catch {
      /* ignore */
    }
  }

  const HTTPS_PLAYGROUND = "https://maxspeech.vercel.app/#playground";

  function insecureOriginHint() {
    return (
      "Browser dictation needs HTTPS (or localhost). Open " +
      HTTPS_PLAYGROUND +
      " to try the mic, or download the desktop app."
    );
  }

  function isInsecureOriginError(err) {
    const msg = String(err?.message || err?.error || err || "");
    return /https|insecure|secure origin|not a secure context/i.test(msg);
  }

  function startListening() {
    if (!window.isSecureContext) {
      if (micHint) micHint.textContent = insecureOriginHint();
      return;
    }
    if (!SpeechRecognition) {
      if (micHint) {
        micHint.textContent =
          "Live mic preview needs Chrome/Edge. Download MaxSpeech for system-wide dictation on Windows, Mac, and Linux.";
      }
      demoType(
        "Thanks for reviewing the build — MaxSpeech types this into any app on your desktop.",
      );
      return;
    }
    if (!recognition) {
      recognition = new SpeechRecognition();
      recognition.continuous = true;
      recognition.interimResults = true;
      recognition.lang = "en-US";
      recognition.onresult = (event) => {
        let interim = "";
        for (let i = event.resultIndex; i < event.results.length; i++) {
          const chunk = event.results[i][0].transcript;
          if (event.results[i].isFinal) finalTranscript += chunk;
          else interim += chunk;
        }
        setPreview(finalTranscript, interim);
      };
      recognition.onerror = (event) => {
        setListeningUI(false);
        if (!micHint) return;
        if (!window.isSecureContext || isInsecureOriginError(event)) {
          micHint.textContent = insecureOriginHint();
          return;
        }
        micHint.textContent =
          "Mic permission blocked or unavailable. You can still browse scenarios below, or download the desktop app.";
      };
      recognition.onend = () => {
        if (listening) {
          try {
            recognition.start();
          } catch (err) {
            setListeningUI(false);
            if (micHint && isInsecureOriginError(err)) {
              micHint.textContent = insecureOriginHint();
            }
          }
        }
      };
    }
    try {
      recognition.start();
      setListeningUI(true);
    } catch (err) {
      setListeningUI(false);
      if (micHint && isInsecureOriginError(err)) {
        micHint.textContent = insecureOriginHint();
      }
    }
  }

  async function demoType(text) {
    setListeningUI(true);
    setPreview("");
    let out = "";
    for (const ch of text) {
      out += ch;
      setPreview(out);
      await new Promise((r) => setTimeout(r, 18 + Math.random() * 28));
      if (!listening && out.length > 8) break;
    }
    setListeningUI(false);
  }

  if (micBtn) {
    const down = (e) => {
      e.preventDefault();
      startListening();
    };
    const up = (e) => {
      e.preventDefault();
      stopListening();
    };
    micBtn.addEventListener("pointerdown", down);
    micBtn.addEventListener("pointerup", up);
    micBtn.addEventListener("pointerleave", () => {
      if (listening) stopListening();
    });
    micBtn.addEventListener("pointercancel", stopListening);
  }

  clearBtn?.addEventListener("click", () => {
    finalTranscript = "";
    setPreview("");
  });

  // —— Scenarios ——
  const tabs = [...document.querySelectorAll(".scenario-tab")];
  const panels = [...document.querySelectorAll("[data-scene-panel]")];

  function typeInto(el) {
    if (!el || el.dataset.typing === "1") return;
    const lines = JSON.parse(el.dataset.lines || "[]");
    const full = lines.join("\n");
    el.dataset.typing = "1";
    el.textContent = "";
    let i = 0;
    const tick = () => {
      if (i > full.length) {
        el.dataset.typing = "0";
        return;
      }
      el.textContent = full.slice(0, i);
      i += 1;
      window.setTimeout(tick, reduceMotion ? 0 : 16 + Math.random() * 22);
    };
    tick();
  }

  function activateScene(name) {
    tabs.forEach((tab) => {
      const on = tab.dataset.scene === name;
      tab.classList.toggle("is-active", on);
      tab.setAttribute("aria-selected", on ? "true" : "false");
    });
    panels.forEach((panel) => {
      const on = panel.getAttribute("data-scene-panel") === name;
      panel.hidden = !on;
      if (on) {
        const target = panel.querySelector(".scenario-type");
        if (target) {
          target.dataset.typing = "0";
          typeInto(target);
        }
      }
    });
  }

  tabs.forEach((tab) => {
    tab.addEventListener("click", () => activateScene(tab.dataset.scene));
  });

  const sceneObserver = new IntersectionObserver(
    (entries) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting) {
          const active = document.querySelector(".scenario-tab.is-active");
          activateScene(active?.dataset.scene || "gmail");
          sceneObserver.disconnect();
        }
      });
    },
    { threshold: 0.35 },
  );
  const scenarioFrame = document.getElementById("scenario-frame");
  if (scenarioFrame) sceneObserver.observe(scenarioFrame);
})();
