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
  normal: 0,
  heart: 1,
  blank: 2,
  angry: 3,
  dizzy: 4,
  "><": 5,
};

// Fraction of the window the model is allowed to fill (lower = smaller).
const MODEL_FIT = 0.55;
// Vertical center as a fraction of window height (lower = higher up).
const MODEL_VERTICAL = 0.32;
// Horizontal center as a fraction of window width (0.5 = centered, lower = left).
const MODEL_HORIZONTAL = 0.3;
// Gap (px) between the speech bubble's tail and the model's head.
const BUBBLE_GAP = 10;
// Nudge the tail anchor down from the model's top edge, as a fraction of the
// model's rendered height, to land on the visible head rather than the
// transparent padding above it (higher = lower on the head).
const BUBBLE_HEAD_INSET = 0.2;
// Hold the >< eye style while hovering; reset it to normal on hover-off.
const HOVER_EXPRESSION_PARAMS: Record<string, number> = { ParamEyeStyle: 4 };
// How much of the model's bounding box counts as "hovered" (1 = full box
// incl. transparent padding, lower = tighter to her body).
const HOVER_HITBOX = 0.7;
// Eye line measured down from the model's rendered top edge. Cursor movement
// crosses from looking up to looking down at this horizon.
const FOCUS_EYE_LINE = 0.3;
// Soften abrupt cursor direction changes before Live2D's focus controller.
const FOCUS_SMOOTHING = 0.12;
// Keep cursor tracking expressive without letting the head reach the rig's
// extreme ±30° focus range.
const MAX_HEAD_TILT_DEGREES = 9;
// Keep the iris from riding into the upper eyelid while preserving full
// downward travel.
const MAX_IRIS_UP = 0.5;

function clampModelParameter(coreModel: any, id: string, maxMagnitude: number) {
  const value = coreModel.getParameterValueById(id);
  coreModel.setParameterValueById(
    id,
    Math.max(-maxMagnitude, Math.min(maxMagnitude, value)),
  );
}

function repositionModel(model: any) {
  const w = app.screen.width;
  const h = app.screen.height;
  // Contain-fit using the model's authored canvas size in PIXELS
  // (internalModel.originalWidth/Height), so wide models don't overflow.
  const im = model.internalModel;
  const mw = im?.originalWidth || 1024;
  const mh = im?.originalHeight || 1024;
  const scale = Math.min(w / mw, h / mh) * MODEL_FIT;
  model.scale.set(scale);
  // Placement on the physical screen is the Tauri window position
  // (tauri.conf.json); this positions the model within that window.
  model.x = w * MODEL_HORIZONTAL;
  model.y = h * MODEL_VERTICAL;
}

async function loadModel() {
  try {
    // Try loading from ~/.sparky/model/ via API server, fall back to bundled
    let modelUrl = "./model/Sparky.model3.json";
    try {
      const resp = await fetch("http://127.0.0.1:3210/model/Sparky.model3.json", { method: "HEAD" });
      if (resp.ok) {
        modelUrl = "http://127.0.0.1:3210/model/Sparky.model3.json";
        console.log("Loading model from ~/.sparky/model/");
      }
    } catch {
      console.log("Loading bundled model");
    }

    const model = await Live2DModel.from(modelUrl, {
      autoInteract: false,
    });

    currentModel = model;
    model.anchor.set(0.5, 0.5);
    const internalModel = model.internalModel as any;
    internalModel.on("beforeModelUpdate", () => {
      const coreModel = internalModel.coreModel;
      clampModelParameter(coreModel, "ParamAngleX", MAX_HEAD_TILT_DEGREES);
      clampModelParameter(coreModel, "ParamAngleY", MAX_HEAD_TILT_DEGREES);
      clampModelParameter(coreModel, "ParamAngleZ", MAX_HEAD_TILT_DEGREES);
      const irisY = coreModel.getParameterValueById("ParamEyeBallY");
      coreModel.setParameterValueById(
        "ParamEyeBallY",
        Math.min(MAX_IRIS_UP, irisY),
      );
    });
    app.stage.addChild(model);
    repositionModel(model);

    window.addEventListener("resize", () => {
      if (currentModel) repositionModel(currentModel);
      if (!bubbleEl.classList.contains("hidden")) positionBubble();
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

// Hover state, driven by the cursor poll below (the window is click-through,
// so DOM hover/mouseenter never fire — we test the global cursor position
// against the model's bounds instead).
let isHovering = false;

// --- Mouse tracking: model looks toward cursor ---
let focusTargetX = 0;
let focusTargetY = 0;
let smoothedFocusX = 0;
let smoothedFocusY = 0;

// Poll cursor position relative to model center (works across monitors)
setInterval(async () => {
  if (!currentModel) return;
  try {
    // Returns logical pixels relative to window origin
    const [cx, cy] = await invoke<[number, number]>("get_cursor_position");

    // Focus around her eyes. The model anchor is centered, so x is already the
    // horizontal center; calculate the eye line down from the rendered top.
    const modelCenterX = currentModel.x;
    const modelEyeY = currentModel.y - currentModel.height / 2 +
      currentModel.height * FOCUS_EYE_LINE;

    // Direction from the eye line to the cursor.
    const dx = cx - modelCenterX;
    const dy = cy - modelEyeY;

    // 300px from model center = full deflection, works for any cursor distance
    const refDist = 300;
    focusTargetX = Math.max(-1, Math.min(1, dx / refDist));
    focusTargetY = Math.max(-1, Math.min(1, -dy / refDist));
    smoothedFocusX += (focusTargetX - smoothedFocusX) * FOCUS_SMOOTHING;
    smoothedFocusY += (focusTargetY - smoothedFocusY) * FOCUS_SMOOTHING;
    // Let Live2D apply focus after the active motion. Its focus controller
    // blends these targets into the eye, head, and body parameters together.
    currentModel.internalModel?.focusController.focus(smoothedFocusX, smoothedFocusY);

    // Hover: keep the full upper bound so her hair is interactive, while
    // tightening the sides and lower bound to avoid transparent padding.
    // (The anchor is 0.5, so currentModel.x/y is the center.)
    const halfW = (currentModel.width / 2) * HOVER_HITBOX;
    const hoverTop = currentModel.y - currentModel.height / 2;
    const hoverBottom = currentModel.y + (currentModel.height / 2) * HOVER_HITBOX;
    const over =
      cx >= currentModel.x - halfW && cx <= currentModel.x + halfW &&
      cy >= hoverTop && cy <= hoverBottom;
    if (over !== isHovering) {
      isHovering = over;
      canvas.classList.toggle("hovered", over);
      // On hover-off, reset the keyforms to neutral once. Live2D params persist
      // frame-to-frame, so a single write sticks (the ticker stops driving them).
      if (!over) {
        const coreModel = (currentModel as any).internalModel?.coreModel;
        if (coreModel) {
          for (const id in HOVER_EXPRESSION_PARAMS) {
            coreModel.setParameterValueById(id, 0);
          }
        }
      }
    }
  } catch {
    // cursor position unavailable
  }
}, 50);

// Apply hover parameters on each frame.
app.ticker.add(() => {
  if (!currentModel) return;
  const coreModel = (currentModel as any).internalModel?.coreModel;
  if (!coreModel) return;

  // While hovering, hold the >< face by driving its eye-style parameter directly
  // (unless she's mid-utterance, so speak expressions win).
  if (isHovering && !lipsyncActive) {
    for (const id in HOVER_EXPRESSION_PARAMS) {
      coreModel.setParameterValueById(id, HOVER_EXPRESSION_PARAMS[id]);
    }
  }
});

// --- Expression & Motion event handlers ---

// How long a triggered expression is held before it fades back to neutral (ms).
const EXPRESSION_HOLD_MS = 4000;
let expressionResetTimer: number | null = null;

// Fade the current expression back to the model's neutral (default) face.
function resetExpression() {
  if (expressionResetTimer !== null) {
    clearTimeout(expressionResetTimer);
    expressionResetTimer = null;
  }
  const em = (currentModel as any)?.internalModel?.motionManager?.expressionManager;
  em?.resetExpression();
}

// Apply an expression by index, then auto-revert to neutral after `holdMs`.
// Pass holdMs = null to hold it open until something else clears it (used by
// speak, which reverts when the audio finishes).
function playExpression(index: number, holdMs: number | null = EXPRESSION_HOLD_MS) {
  if (!currentModel) return;
  if (expressionResetTimer !== null) {
    clearTimeout(expressionResetTimer);
    expressionResetTimer = null;
  }
  currentModel.expression(index);
  if (holdMs !== null) {
    expressionResetTimer = window.setTimeout(() => {
      expressionResetTimer = null;
      resetExpression();
    }, holdMs);
  }
}

listen("api:expression", (event: any) => {
  if (!currentModel) return;
  const { id, name } = event.payload;
  if (id !== undefined) {
    playExpression(id - 1); // API uses 1-based
  } else if (name && name in EXPRESSION_NAMES) {
    playExpression(EXPRESSION_NAMES[name]);
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

// Anchor the bubble's tail just above the model's head and let the box grow
// upward, so it stays glued to the head no matter how short the model is or
// how many lines the text wraps to.
function positionBubble() {
  if (!currentModel) return;
  const headTop = currentModel.y - currentModel.height / 2;
  const anchorY = headTop + currentModel.height * BUBBLE_HEAD_INSET;
  bubbleEl.style.top = "auto";
  bubbleEl.style.bottom = `${window.innerHeight - anchorY + BUBBLE_GAP}px`;
}

function showBubble(text: string, duration: number = 5000) {
  if (bubbleTimer) clearTimeout(bubbleTimer);
  if (typewriterTimer) clearInterval(typewriterTimer);
  bubbleText.textContent = "";
  positionBubble();
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

  if (audio_url) {
    // Hold the expression for the whole utterance, revert when audio ends.
    if (expression !== undefined) playExpression(expression - 1, null);
    // Show bubble, play audio with lip sync, hide bubble when done
    showBubble(text, 999999);
    startLipsync(audio_url, () => {
      hideBubble();
      if (expression !== undefined) resetExpression();
    });
  } else {
    // No audio: hold the expression for the bubble's lifetime.
    const duration = Math.max(text.length * 150, 3000);
    if (expression !== undefined) playExpression(expression - 1, duration);
    showBubble(text, duration);
  }
});
