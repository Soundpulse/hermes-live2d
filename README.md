# ATRI Live2D 桌宠

<p align="center">
  <img src="assets/screenshot.png" width="240" alt="ATRI Live2D 桌宠" />
</p>

<p align="center">
  基于 Tauri 2 + pixi-live2d-display 的 ATRI 桌面宠物，支持语音口型同步、表情切换、鼠标追踪和 HTTP API 控制。
</p>

<p align="center">
  <a href="https://github.com/happy-shine/atri-live2d/releases">Releases</a> ·
  <a href="docs/API.md">API 文档</a> ·
  <a href="examples/claude-code-skill.md">Claude Code Skill 示例</a>
</p>

## 功能

- **透明桌宠** — 无边框、透明背景、置顶显示，可拖拽移动和缩放
- **鼠标追踪** — 模型眼球和头部实时跟随鼠标位置，支持多显示器
- **语音口型同步** — 播放音频时根据音量实时驱动嘴部动画
- **表情与动作** — 19 种表情切换，支持自定义动作播放
- **气泡文字** — 打字机效果的对话气泡，可配置显示时长
- **HTTP API** — 本地 REST API（默认端口 3210），供外部程序驱动桌宠
- **窗口记忆** — 自动保存并恢复窗口位置和大小
- **系统托盘** — 托盘图标控制锁定/解锁和退出

## 配套语音模型

ATRI 的 GPT-SoVITS 语音模型托管在 HuggingFace：

> https://huggingface.co/VoidShine/atri-sovits

配合 TTS 服务使用，可实现日语语音合成 + Live2D 口型同步的完整链路。

## Claude Code 集成

`examples/claude-code-skill.md` 提供了一个 [Claude Code](https://docs.anthropic.com/en/docs/claude-code) Skill 范例，展示如何让 AI 助手在每次回复后自动：

1. 生成 ATRI 风格的日语语音（GPT-SoVITS）
2. 驱动 Live2D 桌宠说话（表情 + 气泡 + 口型同步）

## Hermes Agent 集成

`examples/hermes-skill/atri-live2d-voice/` 提供了一个 [Hermes Agent](https://github.com/NousResearch/hermes-agent) Skill：
运行在远程主机（如 EC2）的 hermes 每次生成 TTS 语音后，通过 Tailscale 将录音以 base64 上传到 `/speak`，
桌宠本地播放并同步口型。详见该目录下的 `SKILL.md`（含 Mac 端 `tailscale serve` 配置步骤）。

## API 速览

桌宠启动后，HTTP API 默认监听 `http://127.0.0.1:3210`。

```bash
# 健康检查
curl http://127.0.0.1:3210/status

# 让 ATRI 说话（文字 + 音频 + 表情）
curl -X POST http://127.0.0.1:3210/speak \
  -H 'Content-Type: application/json' \
  -d '{"text": "高性能ですから！", "audio_url": "file:///path/to/audio.wav", "expression": 13}'

# 切换表情
curl -X POST http://127.0.0.1:3210/expression \
  -H 'Content-Type: application/json' \
  -d '{"name": "YES"}'

# 显示气泡
curl -X POST http://127.0.0.1:3210/bubble \
  -H 'Content-Type: application/json' \
  -d '{"text": "思考中...", "duration": 3000}'
```

完整接口文档见 [docs/API.md](docs/API.md)。

## 配置

配置文件位于 `~/.atri/config.json`，首次启动自动创建：

```json
{
  "api_port": 3210
}
```

窗口状态自动保存至 `~/.atri/window_state.json`。

自定义模型文件放置在 `~/.atri/model/` 目录下，桌宠启动时优先加载该目录中的模型。

## 技术栈

| 层 | 技术 |
|----|------|
| 桌面框架 | [Tauri 2](https://v2.tauri.app) (Rust) |
| 渲染 | [PixiJS 6](https://pixijs.com) + [pixi-live2d-display](https://github.com/guansss/pixi-live2d-display) |
| Live2D | Cubism 4 SDK |
| API 服务 | [Axum](https://github.com/tokio-rs/axum) |
| TTS（可选） | [GPT-SoVITS](https://github.com/RVC-Boss/GPT-SoVITS) |

## 局限性

- **仅支持 macOS** — 使用了 macOS 私有 API（透明窗口、光标穿透等），暂不支持 Windows 和 Linux
- **仅 Apple Silicon (aarch64)** — Release 构建目标为 `aarch64-apple-darwin`
- **Live2D 模型绑定** — 当前硬编码为 ATRI 模型，更换模型需修改代码中的参数映射和表情配置
- **口型同步为音量驱动** — 基于音频频谱音量映射嘴部张合，非 viseme 级别的精确口型
- **API 仅限本地** — HTTP API 监听 `127.0.0.1`，无鉴权，仅供本机进程调用
- **鼠标追踪依赖 Tauri API** — `cursor_position()` 在不同系统版本上的行为可能存在差异

## 许可证

本项目代码以 MIT 许可证发布。

Live2D Cubism SDK 和 ATRI 模型资源的使用需遵守各自的许可协议。
