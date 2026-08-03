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

  const stage = document.getElementById("hero-flow-stage");
  const trailLeft = document.getElementById("flow-trail-left");
  const trailRight = document.getElementById("flow-trail-right");
  const flowActive = document.getElementById("flow-active");
  const flowToast = document.getElementById("flow-toast");
  const flowToastTitle = document.getElementById("flow-toast-title");
  const flowBars = document.getElementById("flow-bars");

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
  const barNodes = flowBars ? [...flowBars.querySelectorAll("i")] : [];

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
    if (!reduceMotion && flowVisible) {
      leftOffset = (leftOffset + 0.012) % 100;
      rightOffset = (rightOffset + 0.01) % 100;
      trailLeft?.setAttribute("startOffset", `${-leftOffset}%`);
      trailRight?.setAttribute("startOffset", `${-rightOffset}%`);

      if (now - lastPhrase > 2800) {
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

      if (now - lastWave > 33 && barNodes.length) {
        const t = now / 1000;
        for (let i = 0; i < barNodes.length; i++) {
          const mid =
            1 - (Math.abs(i - (barNodes.length - 1) / 2) / ((barNodes.length - 1) / 2)) * 0.18;
          const wave =
            0.35 +
            0.45 * Math.sin(t * 5.2 + i * 0.55) +
            0.2 * Math.sin(t * 9.1 + i * 1.1);
          const px = Math.max(2, Math.round(wave * mid * 16));
          barNodes[i].style.height = `${px}px`;
        }
        lastWave = now;
      }
    }
    requestAnimationFrame(tickFlow);
  }

  if (stage) requestAnimationFrame(tickFlow);

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

  function startListening() {
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
      recognition.onerror = () => {
        setListeningUI(false);
        if (micHint) {
          micHint.textContent =
            "Mic permission blocked or unavailable. You can still browse scenarios below, or download the desktop app.";
        }
      };
      recognition.onend = () => {
        if (listening) {
          try {
            recognition.start();
          } catch {
            setListeningUI(false);
          }
        }
      };
    }
    try {
      recognition.start();
      setListeningUI(true);
    } catch {
      setListeningUI(false);
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
