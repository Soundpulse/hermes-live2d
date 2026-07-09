import * as PIXI from "pixi.js";
import { Live2DModel } from "pixi-live2d-display/cubism4";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

(window as any).PIXI = PIXI;
Live2DModel.registerTicker(PIXI.Ticker);

const canvas = document.getElementById("live2d-canvas") as HTMLCanvasElement;
const overlay = document.getElementById("drag-overlay")!;

const app = new PIXI.Application({
  view: canvas,
  backgroundAlpha: 0,
  resizeTo: window,
  antialias: true,
  // Render at the display's native pixel density (Retina) so the model
  // isn't drawn at 1x and upscaled — this is what makes it look soft.
  resolution: window.devicePixelRatio || 1,
  autoDensity: true,
});

let currentModel: any = null;

const EXPRESSION_NAMES: Record<string, number> = {
  "exp1": 0, "exp2": 1, "exp3": 2, "exp4": 3,
  "exp5": 4, "exp6": 5, "exp7": 6,
};

function repositionModel(model: any) {
  const w = app.screen.width;
  const h = app.screen.height;
  // Model-agnostic fit: scale by the model's own canvas height and center it.
  const modelH =
    model.internalModel?.originalHeight || model.height / (model.scale.y || 1);
  const scale = (h * 0.9) / modelH;
  model.scale.set(scale);
  // Centered within the window; on-screen placement is controlled by the
  // Tauri window position (see tauri.conf.json), not the model position.
  model.x = w / 2;
  model.y = h / 2;
}

async function loadModel() {
  try {
    // Try loading from ~/.atri/model/ via API server, fall back to bundled
    let modelUrl = "./model/Murasame.model3.json";
    try {
      const resp = await fetch("http://127.0.0.1:3210/model/Murasame.model3.json", { method: "HEAD" });
      if (resp.ok) {
        modelUrl = "http://127.0.0.1:3210/model/Murasame.model3.json";
        console.log("Loading model from ~/.atri/model/");
      }
    } catch {
      console.log("Loading bundled model");
    }

    const model = await Live2DModel.from(modelUrl, {
      autoInteract: false,
    });

    currentModel = model;
    model.anchor.set(0.5, 0.5);
    app.stage.addChild(model);
    repositionModel(model);

    window.addEventListener("resize", () => {
      if (currentModel) repositionModel(currentModel);
    });
  } catch (e: any) {
    console.error("Failed to load model:", e);
  }
}

// Drag: invoke Rust command directly
overlay.addEventListener("mousedown", async (e) => {
  if (e.button === 0) {
    await invoke("start_drag");
  }
});

// Listen for lock state changes from Rust
listen<boolean>("lock-changed", (event) => {
  const locked = event.payload;
  overlay.style.display = locked ? "none" : "block";
});

loadModel();

// --- Mouse tracking: model looks toward cursor ---
let focusTargetX = 0;
let focusTargetY = 0;
let focusX = 0;
let focusY = 0;

// Poll cursor position relative to model center (works across monitors)
setInterval(async () => {
  if (!currentModel) return;
  try {
    // Returns logical pixels relative to window origin
    const [cx, cy] = await invoke<[number, number]>("get_cursor_position");

    // Model center in CSS pixels (PIXI world space)
    const modelCenterX = currentModel.x + currentModel.width / 2;
    const modelCenterY = currentModel.y + currentModel.height / 2;

    // Direction from model center to cursor
    const dx = cx - modelCenterX;
    const dy = cy - modelCenterY;

    // 300px from model center = full deflection, works for any cursor distance
    const refDist = 300;
    focusTargetX = Math.max(-1, Math.min(1, dx / refDist));
    focusTargetY = Math.max(-1, Math.min(1, -dy / refDist));
  } catch {
    // cursor position unavailable
  }
}, 50);

// Apply focus parameters on each frame after model's internal update
app.ticker.add(() => {
  if (!currentModel) return;
  const coreModel = (currentModel as any).internalModel?.coreModel;
  if (!coreModel) return;

  // Smooth interpolation
  focusX += (focusTargetX - focusX) * 0.15;
  focusY += (focusTargetY - focusY) * 0.15;

  coreModel.setParameterValueById("ParamAngleX", focusX * 30);
  coreModel.setParameterValueById("ParamAngleY", focusY * 30);
  coreModel.setParameterValueById("ParamEyeBallX", focusX);
  coreModel.setParameterValueById("ParamEyeBallY", focusY);
  coreModel.setParameterValueById("ParamBodyAngleX", focusX * 10);
});

// --- Expression & Motion event handlers ---
listen("api:expression", (event: any) => {
  if (!currentModel) return;
  const { id, name } = event.payload;
  if (id !== undefined) {
    currentModel.expression(id - 1); // API uses 1-based
  } else if (name && name in EXPRESSION_NAMES) {
    currentModel.expression(EXPRESSION_NAMES[name]);
  }
});

listen("api:motion", (event: any) => {
  if (!currentModel) return;
  const { group, index } = event.payload;
  currentModel.motion(group, index ?? 0);
});

// --- Bubble text overlay ---
const bubbleEl = document.getElementById("bubble")!;
const bubbleText = document.getElementById("bubble-text")!;
let bubbleTimer: number | null = null;
let typewriterTimer: number | null = null;

function showBubble(text: string, duration: number = 5000) {
  if (bubbleTimer) clearTimeout(bubbleTimer);
  if (typewriterTimer) clearInterval(typewriterTimer);
  bubbleText.textContent = "";
  bubbleEl.classList.remove("hidden");
  let i = 0;
  typewriterTimer = window.setInterval(() => {
    if (i < text.length) {
      bubbleText.textContent += text[i];
      i++;
    } else {
      if (typewriterTimer) clearInterval(typewriterTimer);
    }
  }, 50);
  bubbleTimer = window.setTimeout(() => {
    bubbleEl.classList.add("hidden");
  }, duration);
}

function hideBubble() {
  if (bubbleTimer) clearTimeout(bubbleTimer);
  if (typewriterTimer) clearInterval(typewriterTimer);
  bubbleEl.classList.add("hidden");
}

// Expose for use by other modules (Task 5)
(window as any).showBubble = showBubble;
(window as any).hideBubble = hideBubble;

listen("api:bubble", (event: any) => {
  const { text, duration } = event.payload;
  showBubble(text, duration ?? 5000);
});

// --- Audio playback and lip sync ---
let audioContext: AudioContext | null = null;
let currentAudio: HTMLAudioElement | null = null;
let analyserNode: AnalyserNode | null = null;
let lipsyncActive = false;

function resolveAudioUrl(url: string): string {
  // Convert file:// URLs and absolute paths to HTTP proxy
  if (url.startsWith("file:///")) {
    const path = url.slice(7); // remove file://
    return `http://127.0.0.1:3210/audio?path=${encodeURIComponent(path)}`;
  }
  if (url.startsWith("/")) {
    return `http://127.0.0.1:3210/audio?path=${encodeURIComponent(url)}`;
  }
  return url;
}

function startLipsync(audioUrl: string, onEnd?: () => void) {
  stopLipsync();
  audioUrl = resolveAudioUrl(audioUrl);

  if (!audioContext) {
    audioContext = new AudioContext();
  }

  // Resume AudioContext if suspended (autoplay policy)
  if (audioContext.state === "suspended") {
    audioContext.resume();
  }

  const audio = new Audio();
  audio.crossOrigin = "anonymous";
  audio.src = audioUrl;
  currentAudio = audio;

  const source = audioContext.createMediaElementSource(audio);
  const analyser = audioContext.createAnalyser();
  analyser.fftSize = 256;
  source.connect(analyser);
  analyser.connect(audioContext.destination);
  analyserNode = analyser;
  lipsyncActive = true;

  const dataArray = new Uint8Array(analyser.frequencyBinCount);

  function updateMouth() {
    if (!lipsyncActive || !analyserNode) return;
    analyserNode.getByteFrequencyData(dataArray);

    let sum = 0;
    for (let i = 0; i < 32; i++) sum += dataArray[i];
    const volume = sum / 32 / 255;

    if (currentModel) {
      const coreModel = (currentModel as any).internalModel?.coreModel;
      if (coreModel) {
        coreModel.setParameterValueById("ParamMouthOpenY", volume * 1.2);
      }
    }

    requestAnimationFrame(updateMouth);
  }

  audio.addEventListener("ended", () => {
    stopLipsync();
    if (onEnd) onEnd();
  });

  audio.addEventListener("error", (_e) => {
    console.error("Audio error:", audio.error);
    stopLipsync();
    if (onEnd) onEnd();
  });

  audio.play().then(() => {
    console.log("Audio playing:", audioUrl);
    updateMouth();
  }).catch((e) => {
    console.error("Audio play failed:", e);
    stopLipsync();
    if (onEnd) onEnd();
  });
}

function stopLipsync() {
  lipsyncActive = false;
  if (currentAudio) {
    currentAudio.pause();
    currentAudio = null;
  }
  analyserNode = null;
  if (currentModel) {
    const coreModel = (currentModel as any).internalModel?.coreModel;
    if (coreModel) {
      coreModel.setParameterValueById("ParamMouthOpenY", 0);
    }
  }
}

listen("api:lipsync:start", (event: any) => {
  const { audio_url } = event.payload;
  startLipsync(audio_url);
});

listen("api:lipsync:stop", () => {
  stopLipsync();
});

// --- Speak endpoint (combined) ---
listen("api:speak", (event: any) => {
  if (!currentModel) return;
  const { text, audio_url, expression } = event.payload;

  // Set expression if provided
  if (expression !== undefined) {
    currentModel.expression(expression - 1);
  }

  if (audio_url) {
    // Show bubble, play audio with lip sync, hide bubble when done
    showBubble(text, 999999);
    startLipsync(audio_url, () => {
      hideBubble();
    });
  } else {
    // No audio: just show bubble with calculated duration
    showBubble(text, Math.max(text.length * 150, 3000));
  }
});
