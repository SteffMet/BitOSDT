use crate::build::winpe_ui::generate_kiosk_helper_ps1;

fn js_string_literal(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| {
        let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{}\"", escaped)
    })
}

pub fn generate_provisioning_kiosk_helper_ps1(log_path: &str, window_title: &str) -> String {
    generate_kiosk_helper_ps1()
        .replace(r#"X:\BitOSDT\Logs\shell-launch.log"#, log_path)
        .replace("BitOSDT Deployment", window_title)
}

pub fn generate_provisioning_hta(
    profile_path: &str,
    state_path: &str,
    status_path: &str,
    app_progress_path: &str,
    command_path: &str,
    controller_path: &str,
    heartbeat_path: &str,
    shell_log_path: &str,
) -> String {
    let mut html = r##"<html>
<head>
  <title>BitOSDT Provisioning</title>
  <HTA:APPLICATION
    APPLICATIONNAME="BitOSDT Provisioning"
    BORDER="none"
    CAPTION="no"
    SHOWINTASKBAR="yes"
    SINGLEINSTANCE="yes"
    SYSMENU="no"
    SCROLL="no"
    WINDOWSTATE="maximize"
  />
  <meta http-equiv="X-UA-Compatible" content="IE=edge" />
  <style>
    html, body {
      margin: 0;
      padding: 0;
      height: 100%;
      width: 100%;
      overflow: hidden;
      font-family: "Segoe UI", Tahoma, Arial, sans-serif;
      background: radial-gradient(circle at top left, #172554 0%, #020617 62%, #01020a 100%);
      color: #e2e8f0;
    }
    * { box-sizing: border-box; }
    body { position: relative; min-height: 100%; }
    #shell {
      position: absolute;
      top: 0;
      right: 0;
      bottom: 0;
      left: 0;
      padding: 24px;
      overflow: hidden;
    }
    .panel {
      position: absolute;
      background: rgba(15, 23, 42, 0.78);
      border: 1px solid rgba(148, 163, 184, 0.18);
      border-radius: 24px;
      box-shadow: 0 28px 70px rgba(15, 23, 42, 0.38);
    }
    #mainPanel {
      left: 24px;
      top: 24px;
      right: 404px;
      bottom: 24px;
      display: -ms-flexbox;
      display: flex;
      -ms-flex-direction: column;
      flex-direction: column;
      padding: 28px 30px;
      min-width: 0;
      min-height: 0;
      overflow: hidden;
    }
    #sidePanel {
      top: 24px;
      right: 24px;
      bottom: 24px;
      width: 356px;
      display: -ms-flexbox;
      display: flex;
      -ms-flex-direction: column;
      flex-direction: column;
      padding: 24px;
      min-width: 0;
      min-height: 0;
      overflow: auto;
    }
    #sidePanel > div { margin-bottom: 18px; }
    #sidePanel > div:last-child {
      margin-bottom: 0;
      -ms-flex: 1 1 auto;
      flex: 1 1 auto;
      min-height: 0;
    }
    .eyebrow {
      text-transform: uppercase;
      font-size: 12px;
      letter-spacing: 0.18em;
      color: #93c5fd;
      margin-bottom: 8px;
    }
    #title {
      font-size: 40px;
      font-weight: 700;
      line-height: 1.1;
      margin: 0 0 8px 0;
    }
    #subtitle, #detailText {
      color: #cbd5e1;
      font-size: 16px;
      line-height: 1.5;
    }
    #statusBanner {
      margin-top: 18px;
      padding: 14px 16px;
      border-radius: 16px;
      font-size: 14px;
      line-height: 1.45;
      display: none;
    }
    #statusBanner.info {
      display: block;
      background: rgba(37, 99, 235, 0.14);
      border: 1px solid rgba(96, 165, 250, 0.32);
      color: #dbeafe;
    }
    #statusBanner.error {
      display: block;
      background: rgba(127, 29, 29, 0.4);
      border: 1px solid rgba(248, 113, 113, 0.38);
      color: #fee2e2;
    }
    #stepCard {
      margin-top: 24px;
      border-radius: 22px;
      background: rgba(2, 6, 23, 0.4);
      border: 1px solid rgba(71, 85, 105, 0.36);
      padding: 22px;
      min-height: 0;
      -ms-flex: 1 1 auto;
      flex: 1 1 auto;
      overflow: auto;
    }
    .stepTitle {
      font-size: 28px;
      font-weight: 700;
      margin-bottom: 10px;
    }
    .stepBody {
      color: #cbd5e1;
      font-size: 15px;
      line-height: 1.55;
    }
    .fieldLabel {
      display: block;
      margin: 18px 0 8px 0;
      color: #bfdbfe;
      font-size: 13px;
      font-weight: 600;
      letter-spacing: 0.04em;
      text-transform: uppercase;
    }
    .textInput {
      width: 100%;
      border-radius: 14px;
      border: 1px solid rgba(148, 163, 184, 0.28);
      background: rgba(15, 23, 42, 0.9);
      color: #f8fafc;
      font-size: 16px;
      padding: 14px 16px;
      outline: none;
    }
    .textInput:focus {
      border-color: rgba(96, 165, 250, 0.7);
      box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.16);
    }
    .inputHint {
      font-size: 13px;
      color: #94a3b8;
      margin-top: 8px;
    }
    .choiceRow {
      margin-top: 18px;
      display: -ms-flexbox;
      display: flex;
      -ms-flex-align: center;
      align-items: center;
      padding: 14px 16px;
      border-radius: 16px;
      background: rgba(30, 41, 59, 0.58);
      border: 1px solid rgba(100, 116, 139, 0.24);
    }
    .choiceRow input { margin-right: 12px; }
    .choiceRow input { transform: scale(1.2); }
    .choiceText { font-size: 15px; color: #e2e8f0; }
    #appProgressCard {
      margin-top: 20px;
      padding: 18px;
      border-radius: 18px;
      background: rgba(15, 23, 42, 0.82);
      border: 1px solid rgba(71, 85, 105, 0.32);
      display: none;
    }
    #appProgressTitle {
      font-size: 14px;
      color: #bfdbfe;
      text-transform: uppercase;
      letter-spacing: 0.08em;
      margin-bottom: 8px;
    }
    #appProgressCurrent {
      font-size: 22px;
      font-weight: 700;
      margin-bottom: 6px;
    }
    #appProgressCount {
      font-size: 14px;
      color: #cbd5e1;
      margin-bottom: 12px;
    }
    .progressTrack {
      height: 12px;
      width: 100%;
      border-radius: 999px;
      overflow: hidden;
      background: rgba(30, 41, 59, 0.95);
      border: 1px solid rgba(96, 165, 250, 0.18);
    }
    .progressFill {
      height: 100%;
      width: 0%;
      border-radius: 999px;
      background: linear-gradient(90deg, #2563eb 0%, #38bdf8 100%);
      transition: width 0.25s ease;
    }
    #actions {
      display: -ms-flexbox;
      display: flex;
      -ms-flex-direction: column;
      flex-direction: column;
      align-items: flex-start;
      margin-top: 22px;
    }
    #actionText { margin-bottom: 16px; }
    #actionButtonRow {
      width: 100%;
    }
    #nextButton {
      min-width: 170px;
      border: none;
      border-radius: 16px;
      background: linear-gradient(135deg, #2563eb 0%, #38bdf8 100%);
      color: white;
      font-size: 18px;
      font-weight: 700;
      padding: 15px 22px;
      cursor: pointer;
      box-shadow: 0 16px 32px rgba(37, 99, 235, 0.24);
    }
    #nextButton:disabled {
      cursor: default;
      opacity: 0.55;
      box-shadow: none;
    }
    #stepMeta {
      color: #93c5fd;
      font-size: 13px;
      text-transform: uppercase;
      letter-spacing: 0.08em;
    }
    #profileName {
      font-size: 22px;
      font-weight: 700;
      margin-bottom: 4px;
    }
    #profileDescription {
      color: #94a3b8;
      font-size: 13px;
      line-height: 1.45;
    }
    .sectionTitle {
      font-size: 13px;
      text-transform: uppercase;
      letter-spacing: 0.1em;
      color: #93c5fd;
      margin-bottom: 10px;
    }
    .pillGrid {
      display: flex;
      flex-wrap: wrap;
    }
    .pill {
      margin: 0 8px 8px 0;
      border-radius: 999px;
      padding: 7px 10px;
      background: rgba(30, 41, 59, 0.78);
      border: 1px solid rgba(71, 85, 105, 0.36);
      font-size: 12px;
      color: #dbeafe;
    }
    #taskList, #settingsList {
      list-style: none;
      margin: 0;
      padding: 0;
      display: block;
    }
    .taskItem, .settingItem {
      display: -ms-flexbox;
      display: flex;
      -ms-flex-align: start;
      align-items: flex-start;
      justify-content: space-between;
      padding: 12px 14px;
      border-radius: 16px;
      background: rgba(15, 23, 42, 0.52);
      border: 1px solid rgba(71, 85, 105, 0.22);
    }
    .taskItem > div, .settingItem > div { margin-right: 12px; }
    .taskItem, .settingItem { margin-bottom: 10px; }
    #taskList li:last-child, #settingsList li:last-child { margin-bottom: 0; }
    .taskLabel, .settingLabel {
      font-size: 14px;
      font-weight: 600;
      color: #e2e8f0;
    }
    .taskDetail, .settingValue {
      margin-top: 4px;
      font-size: 12px;
      color: #94a3b8;
    }
    .taskState {
      min-width: 84px;
      min-height: 32px;
      padding: 0 10px;
      border-radius: 999px;
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 6px;
      font-size: 11px;
      font-weight: 700;
      letter-spacing: 0.06em;
      text-transform: uppercase;
      white-space: nowrap;
      color: #f8fafc;
      background: rgba(71, 85, 105, 0.45);
      border: 1px solid rgba(100, 116, 139, 0.32);
    }
    .taskStateGlyph {
      display: inline-block;
      min-width: 14px;
      text-align: center;
      font-family: Consolas, "Lucida Console", monospace;
      font-size: 12px;
      line-height: 1;
    }
    .taskStateText {
      display: inline-block;
      line-height: 1;
    }
    .taskState.complete { background: rgba(22, 101, 52, 0.75); border-color: rgba(74, 222, 128, 0.34); }
    .taskState.active { background: rgba(30, 64, 175, 0.8); border-color: rgba(96, 165, 250, 0.36); }
    .taskState.failed { background: rgba(127, 29, 29, 0.82); border-color: rgba(248, 113, 113, 0.38); }
    .taskState.reboot_pending, .taskState.rebootPending { background: rgba(133, 77, 14, 0.82); border-color: rgba(251, 191, 36, 0.36); }
    .taskState.pending { background: rgba(51, 65, 85, 0.7); }
    #footerNote {
      margin-top: 12px;
      font-size: 12px;
      color: #64748b;
    }
    #actionText {
      max-width: 620px;
    }
    #shell.compact {
      padding: 14px;
    }
    #shell.compact #mainPanel {
      left: 14px;
      top: 14px;
      right: 14px;
      bottom: 46%;
      padding: 18px;
    }
    #shell.compact #sidePanel {
      left: 14px;
      right: 14px;
      top: 56%;
      bottom: 14px;
      width: auto;
      padding: 18px;
    }
    #shell.compact .panel, #shell.compact #stepCard {
      border-radius: 18px;
    }
    #shell.compact #title {
      font-size: 28px;
    }
    #shell.compact #subtitle,
    #shell.compact #detailText,
    #shell.compact .stepBody,
    #shell.compact .choiceText,
    #shell.compact .textInput {
      font-size: 14px;
    }
    #shell.compact .stepTitle {
      font-size: 24px;
    }
    #shell.compact #stepCard {
      margin-top: 18px;
      padding: 18px;
    }
    #shell.compact #actions {
      margin-top: 18px;
    }
    #shell.compact #nextButton {
      width: 100%;
      min-width: 0;
    }
    #shell.compact #sidePanel > div {
      margin-bottom: 14px;
    }
  </style>
</head>
<body>
  <div id="shell">
    <div id="mainPanel" class="panel">
      <div>
        <div class="eyebrow">Provisioning Package</div>
        <div id="title">Preparing BitOSDT provisioning...</div>
        <div id="subtitle">The kiosk shell is waiting for provisioning state.</div>
        <div id="statusBanner"></div>
      </div>
      <div id="detailText"></div>
      <div id="stepCard">
        <div class="stepTitle" id="stepTitle">Loading provisioning flow</div>
        <div class="stepBody" id="stepDescription">Waiting for status updates...</div>
        <div id="dynamicInput"></div>
        <div id="appProgressCard">
          <div id="appProgressTitle">Applications</div>
          <div id="appProgressCurrent">Waiting to start</div>
          <div id="appProgressCount">0 / 0</div>
          <div class="progressTrack"><div class="progressFill" id="appProgressFill"></div></div>
        </div>
      </div>
      <div id="actions">
        <div id="actionText">
          <div id="stepMeta">BitOSDT Provisioning Mode</div>
          <div id="footerNote">If a step needs a reboot, BitOSDT resumes here automatically.</div>
        </div>
        <div id="actionButtonRow"><button id="nextButton" onclick="submitCurrentStep()">Next</button></div>
      </div>
    </div>
    <div id="sidePanel" class="panel">
      <div>
        <div class="sectionTitle">Profile</div>
        <div id="profileName">Provisioning profile</div>
        <div id="profileDescription"></div>
      </div>
      <div>
        <div class="sectionTitle">Enabled Settings</div>
        <ul id="settingsList"></ul>
      </div>
      <div>
        <div class="sectionTitle">Tasks</div>
        <ul id="taskList"></ul>
      </div>
    </div>
  </div>

  <script language="javascript">
    var profilePath = __PROFILE_PATH__;
    var statePath = __STATE_PATH__;
    var statusPath = __STATUS_PATH__;
    var appProgressPath = __APP_PROGRESS_PATH__;
    var commandPath = __COMMAND_PATH__;
    var controllerPath = __CONTROLLER_PATH__;
    var heartbeatPath = __HEARTBEAT_PATH__;
    var shellLogPath = __SHELL_LOG_PATH__;
    var tickIntervalMs = 1200;
    var missingFileRetryMs = 2500;
    var lastCommandAt = 0;
    var fileSystemObject = null;
    var fileReadCache = {};
    var dataFallbackState = {};
    var lastGoodData = {
      profile: null,
      state: null,
      status: null,
      appProgress: null
    };
    var lastRenderSignatures = {
      profile: "",
      status: "",
      main: ""
    };
    var heartbeatState = {
      schemaVersion: 1,
      lastTickUtc: "",
      lastRenderUtc: "",
      tickDurationMs: 0,
      lastError: "",
      inTick: false
    };
    var tickTimer = null;
    var tickInFlight = false;
    var lastTickLogAt = 0;

    function errorMessage(error) {
      if (error && error.message) { return String(error.message); }
      if (error && error.description) { return String(error.description); }
      return String(error);
    }

    function appendShellLogWithFso(fso, message) {
      var folderPath = shellLogPath.substring(0, shellLogPath.lastIndexOf("\\"));
      if (folderPath && !fso.FolderExists(folderPath)) {
        ensureFolderPath(folderPath);
      }
      var stream = fso.OpenTextFile(shellLogPath, 8, true, 0);
      var stamp = new Date().toUTCString();
      stream.WriteLine(stamp + " " + message);
      stream.Close();
    }

    function getFileSystemObject() {
      if (fileSystemObject === null) {
        fileSystemObject = new ActiveXObject("Scripting.FileSystemObject");
        appendShellLogWithFso(fileSystemObject, "Created FileSystemObject.");
      }
      return fileSystemObject;
    }

    function sanitizeText(text) {
      if (text === null || typeof text === "undefined") { return ""; }
      var clean = String(text);
      clean = clean.replace(/^\u00EF\u00BB\u00BF/, "");
      clean = clean.replace(/\u00EF\u00BB\u00BF/g, "");
      clean = clean.replace(/^\uFEFF/, "");
      clean = clean.replace(/\u0000/g, "");
      return clean;
    }

    function ensureFolderPath(path) {
      var fso = getFileSystemObject();
      if (!path || fso.FolderExists(path)) { return; }
      var parent = path.substring(0, path.lastIndexOf("\\"));
      if (parent && !fso.FolderExists(parent)) {
        ensureFolderPath(parent);
      }
      if (!fso.FolderExists(path)) {
        fso.CreateFolder(path);
      }
    }

    function appendShellLog(message) {
      try {
        var fso = fileSystemObject;
        if (fso === null) {
          fso = new ActiveXObject("Scripting.FileSystemObject");
          fileSystemObject = fso;
        }
        appendShellLogWithFso(fso, message);
      } catch (error) {
      }
    }

    function readFileText(path) {
      var now = new Date().getTime();
      var cache = fileReadCache[path];
      try {
        var fso = getFileSystemObject();
        if (!fso.FileExists(path)) {
          if (cache && cache.missing && (now - cache.checkedAt) < missingFileRetryMs) {
            return cache.text;
          }
          fileReadCache[path] = {
            checkedAt: now,
            missing: true,
            size: 0,
            stamp: "",
            text: ""
          };
          return "";
        }

        var file = fso.GetFile(path);
        var stamp = "";
        var size = 0;
        try { stamp = String(file.DateLastModified); } catch (ignoreStamp) {}
        try { size = file.Size; } catch (ignoreSize) {}

        if (cache && !cache.missing && cache.stamp === stamp && cache.size === size) {
          cache.checkedAt = now;
          return cache.text;
        }

        var stream = fso.OpenTextFile(path, 1, false);
        var text = stream.ReadAll();
        stream.Close();
        text = sanitizeText(text);
        fileReadCache[path] = {
          checkedAt: now,
          missing: false,
          size: size,
          stamp: stamp,
          text: text
        };
        return text;
      } catch (error) {
        if (cache) {
          cache.checkedAt = now;
          return cache.text;
        }
        return "";
      }
    }

    function writeFileText(path, text) {
      var fso = getFileSystemObject();
      var folderPath = path.substring(0, path.lastIndexOf("\\"));
      if (folderPath && !fso.FolderExists(folderPath)) {
        ensureFolderPath(folderPath);
      }
      var stream = fso.OpenTextFile(path, 2, true, 0);
      stream.Write(text || "");
      stream.Close();
      delete fileReadCache[path];
    }

    function parseJson(text) {
      text = sanitizeText(text);
      if (!text || !text.replace(/\s+/g, "").length) { return null; }
      try { return JSON.parse(text); } catch (error) {
        try { return eval("(" + text + ")"); } catch (ignore) { return null; }
      }
    }

    function htmlEncode(value) {
      if (value === null || value === undefined) { return ""; }
      return String(value)
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;");
    }

    function signatureFor(value) {
      if (value === null || typeof value === "undefined") { return ""; }
      try {
        return JSON.stringify(value);
      } catch (error) {
        return String(value);
      }
    }

    function replaceHtmlIfChanged(element, html) {
      if (!element) { return false; }
      if (element.innerHTML === html) { return false; }
      element.innerHTML = html;
      return true;
    }

    function setTextIfChanged(id, value) {
      var element = document.getElementById(id);
      if (!element) { return false; }
      var next = value || "";
      if (element.innerText === next) { return false; }
      element.innerText = next;
      return true;
    }

    function noteDataFallback(key, message) {
      if (dataFallbackState[key] === message) { return; }
      dataFallbackState[key] = message;
      appendShellLog(message);
    }

    function clearDataFallback(key) {
      if (!dataFallbackState[key]) { return; }
      appendShellLog("Recovered " + key + " data from source file.");
      dataFallbackState[key] = "";
    }

    function readJsonDocument(path, key) {
      var text = readFileText(path);
      var parsed = parseJson(text);
      if (parsed !== null) {
        lastGoodData[key] = parsed;
        clearDataFallback(key);
        return parsed;
      }

      var cleaned = sanitizeText(text);
      var hasText = cleaned && cleaned.replace(/\s+/g, "").length;
      if (hasText) {
        noteDataFallback(key, "Using cached " + key + " data after JSON parse issue at " + path + ".");
        return lastGoodData[key];
      }
      if (lastGoodData[key] !== null) {
        noteDataFallback(key, "Using cached " + key + " data while source file is unavailable: " + path + ".");
        return lastGoodData[key];
      }

      noteDataFallback(key, "No " + key + " data available yet from " + path + ".");
      return null;
    }

    function writeHeartbeat() {
      try {
        writeFileText(heartbeatPath, JSON.stringify(heartbeatState, null, 2));
      } catch (error) {
        appendShellLog("Failed to update UI heartbeat: " + errorMessage(error));
      }
    }

    function normalizeTaskStatus(status) {
      if (!status) { return "pending"; }
      if (status === "rebootPending") { return "reboot_pending"; }
      return String(status);
    }

    function taskStatusInfo(status) {
      var normalized = normalizeTaskStatus(status);
      if (normalized === "complete") { return { glyph: "OK", label: "Done" }; }
      if (normalized === "active") { return { glyph: "..", label: "Live" }; }
      if (normalized === "failed") { return { glyph: "!", label: "Error" }; }
      if (normalized === "reboot_pending") { return { glyph: "R", label: "Reboot" }; }
      return { glyph: "-", label: "Waiting" };
    }

    function taskStatusMarkup(status) {
      var info = taskStatusInfo(status);
      return "<span class='taskStateGlyph'>" +
        htmlEncode(info.glyph) +
        "</span><span class='taskStateText'>" +
        htmlEncode(info.label) +
        "</span>";
    }

    function focusComputerNameInput(selectAll) {
      var input = document.getElementById("computerNameInput");
      if (!input) { return; }
      try {
        input.focus();
        if (selectAll) {
          if (typeof input.select === "function") {
            input.select();
          } else if (input.createTextRange) {
            var range = input.createTextRange();
            range.moveStart("character", 0);
            range.moveEnd("character", input.value.length);
            range.select();
          }
        }
      } catch (error) {
      }
    }

    function statusBanner(kind, text) {
      var el = document.getElementById("statusBanner");
      if (!text) {
        el.className = "";
        el.style.display = "none";
        el.innerText = "";
        return;
      }
      el.className = kind || "info";
      el.style.display = "block";
      el.innerText = text;
    }

    function renderSettings(profile) {
      var el = document.getElementById("settingsList");
      if (!profile) {
        replaceHtmlIfChanged(el, "<li class='settingItem'><div><div class='settingLabel'>Waiting for profile</div><div class='settingValue'>Provisioning snapshot not found yet.</div></div></li>");
        return;
      }

      setTextIfChanged("profileName", profile.name || "Provisioning profile");
      setTextIfChanged("profileDescription", profile.description || "Interactive provisioning wizard");

      var items = [
        { label: "Language / Region", value: profile.language || "Not set" },
        { label: "Input Locale", value: profile.inputLocale || "Not set" },
        { label: "Timezone", value: profile.timezone || "Not set" },
        { label: "Skip Machine OOBE", value: profile.skipMachineOobe ? "Enabled" : "Disabled" },
        { label: "Skip User OOBE", value: profile.skipUserOobe ? "Enabled" : "Disabled" },
        { label: "Hide EULA", value: profile.hideEula ? "Enabled" : "Disabled" },
        { label: "Hide Privacy Settings", value: profile.hidePrivacySettings ? "Enabled" : "Disabled" },
        { label: "Hide Online Account Screens", value: profile.hideOnlineAccountScreens ? "Enabled" : "Disabled" },
        { label: "Hide Wireless Setup", value: profile.hideWirelessSetup ? "Enabled" : "Disabled" },
        { label: "Default User", value: profile.defaultUserEnabled ? "Enabled" : "Disabled" },
        { label: "Domain Join", value: profile.domainJoinEnabled ? "Enabled" : "Disabled" },
        { label: "Wi-Fi", value: profile.wifiEnabled ? "Enabled" : "Disabled" },
        { label: "Disable BitLocker", value: profile.disableBitLocker ? "Enabled" : "Disabled" },
        {
          label: "BitLocker Restart",
          value: profile.disableBitLocker
            ? (profile.rebootAfterDisableBitLocker ? "Restart after disable" : "Continue without restart")
            : "Disabled"
        },
        {
          label: "Wi-Fi DNS",
          value: profile.wifiEnabled
            ? ((profile.wifiDnsServers && profile.wifiDnsServers.length)
              ? profile.wifiDnsServers.join(", ")
              : "Automatic")
            : "Disabled"
        },
        { label: "Apps", value: (profile.appItemCount || 0) + " item(s)" }
      ];

      var html = "";
      for (var i = 0; i < items.length; i++) {
        html += "<li class='settingItem'><div><div class='settingLabel'>" +
          htmlEncode(items[i].label) +
          "</div><div class='settingValue'>" +
          htmlEncode(items[i].value) +
          "</div></div></li>";
      }
      replaceHtmlIfChanged(el, html);
    }

    function renderTasks(status) {
      var el = document.getElementById("taskList");
      if (!status || !status.tasks || !status.tasks.length) {
        replaceHtmlIfChanged(
          el,
          "<li class='taskItem'><div><div class='taskLabel'>Waiting for tasks</div><div class='taskDetail'>Provisioning has not started yet.</div></div><span class='taskState pending'>" + taskStatusMarkup("pending") + "</span></li>"
        );
        return;
      }

      var html = "";
      for (var i = 0; i < status.tasks.length; i++) {
        var task = status.tasks[i];
        var cssStatus = normalizeTaskStatus(task.status);
        html += "<li class='taskItem'><div><div class='taskLabel'>" +
          htmlEncode(task.title || task.id) +
          "</div><div class='taskDetail'>" +
          htmlEncode(task.detail || "") +
          "</div></div><span class='taskState " +
          htmlEncode(cssStatus) + "'>" + taskStatusMarkup(cssStatus) + "</span></li>";
      }
      replaceHtmlIfChanged(el, html);
    }

    function renderMain(profile, state, status, appProgress) {
      var title = "BitOSDT Provisioning";
      var subtitle = "Guided provisioning wizard";
      var detail = "";
      var stepTitle = "Loading";
      var stepDescription = "Waiting for provisioning status...";
      var dynamicHtml = "";
      var nextButton = document.getElementById("nextButton");
      nextButton.disabled = false;
      nextButton.innerText = "Next";
      nextButton.style.display = "inline-block";

      if (status && status.terminalStatus === "complete") {
        title = "Provisioning complete";
        subtitle = "All enabled BitOSDT tasks have finished.";
        stepTitle = "Completed";
        stepDescription = "The configured steps are finished. You can close this window.";
        dynamicHtml = "";
        nextButton.style.display = "none";
      } else if (state && state.currentStepId === "computerName") {
        title = "Computer name";
        subtitle = "Confirm the device name before continuing.";
        detail = "BitOSDT can rename the PC now and resume automatically after restart.";
        stepTitle = "Enter PC name";
        stepDescription = "Use letters, numbers, and hyphens only. Maximum 15 characters.";
        var value = (state && state.computerName) ? state.computerName : ((profile && profile.explicitComputerName) ? profile.explicitComputerName : "");
        dynamicHtml =
          "<label class='fieldLabel' for='computerNameInput'>Computer Name</label>" +
          "<input id='computerNameInput' class='textInput' maxlength='15' value='" + htmlEncode(value) + "' />" +
          "<div class='inputHint'>Only letters, numbers, and hyphens are allowed.</div>" +
          renderRestartToggle(state, "computerName");
      } else if (state && state.currentStepId === "wifi") {
        title = "Wi-Fi settings";
        subtitle = "Apply wireless connectivity";
        detail = "BitOSDT will apply the saved Wi-Fi profile and continue once the step completes.";
        stepTitle = "Apply Wi-Fi settings";
        stepDescription = profile && profile.wifiSsid
          ? ("SSID: " + profile.wifiSsid + ((profile.wifiDnsServers && profile.wifiDnsServers.length) ? (" / DNS: " + profile.wifiDnsServers.join(", ")) : ""))
          : "Wireless profile is enabled for this package.";
        dynamicHtml = renderRestartToggle(state, "wifi");
      } else if (state && state.currentStepId === "domainJoin") {
        title = "Domain join";
        subtitle = "Join the device to your domain";
        detail = "Domain join usually needs a restart before the next step.";
        stepTitle = "Join domain";
        stepDescription = profile && profile.domainName ? ("Target domain: " + profile.domainName) : "BitOSDT will run the packaged domain join script.";
        dynamicHtml = renderRestartToggle(state, "domainJoin");
      } else if (state && state.currentStepId === "bitLocker") {
        title = "BitLocker";
        subtitle = "Disable protection before app installs";
        detail = profile && profile.rebootAfterDisableBitLocker
          ? "This profile restarts automatically after BitLocker is disabled, then resumes with applications."
          : "This profile continues straight to application installs after BitLocker is disabled.";
        stepTitle = "Disable BitLocker on C:";
        stepDescription = "BitOSDT runs manage-bde.exe -off C: and treats an already unprotected or decrypting drive as complete.";
        dynamicHtml = renderBitLockerStep(profile);
      } else if (state && state.currentStepId === "apps") {
        title = "Applications";
        subtitle = "Install packaged software";
        detail = "Progress updates are shown live as BitOSDT runs each package.";
        stepTitle = "Install applications";
        stepDescription = "BitOSDT installs the enabled software packages and custom installers for this profile.";
        dynamicHtml = renderRestartToggle(state, "apps");
        renderAppProgress(appProgress, true);
      } else if (state && state.currentStepId === "optionalScripts") {
        title = "Custom actions";
        subtitle = "Run debloat and custom scripts";
        detail = "BitOSDT will finish the remaining scripted actions for this package.";
        stepTitle = "Run remaining scripts";
        stepDescription = "This includes debloat actions and any enabled custom scripts.";
        dynamicHtml = renderRestartToggle(state, "optionalScripts");
      } else {
        title = "Provisioning ready";
        subtitle = "BitOSDT is preparing the guided flow.";
        detail = "The provisioning host is starting the kiosk session.";
        stepTitle = "Preparing session";
        stepDescription = "Waiting for the local state files to appear.";
      }

      if (!(state && state.currentStepId === "apps")) {
        renderAppProgress(appProgress, false);
      }

      if (status && status.errorMessage) {
        statusBanner("error", status.errorMessage);
      } else if (status && status.bannerMessage) {
        statusBanner("info", status.bannerMessage);
      } else if (state && state.rebootPending) {
        statusBanner("info", "A restart is pending. BitOSDT resumes automatically after sign-in.");
      } else {
        statusBanner("", "");
      }

      if (state && state.inProgress) {
        nextButton.disabled = true;
        nextButton.innerText = "Applying...";
      }

      var dynamicReplaced = replaceHtmlIfChanged(document.getElementById("dynamicInput"), dynamicHtml);
      if (dynamicReplaced && state && state.currentStepId === "computerName" && !state.inProgress) {
        window.setTimeout(function() { focusComputerNameInput(true); }, 30);
      }
      setTextIfChanged("title", title);
      setTextIfChanged("subtitle", subtitle);
      setTextIfChanged("detailText", detail);
      setTextIfChanged("stepTitle", stepTitle);
      setTextIfChanged("stepDescription", stepDescription);
      setTextIfChanged(
        "stepMeta",
        status && status.percentComplete !== undefined
          ? ("Progress " + status.percentComplete + "%")
          : "BitOSDT Provisioning Mode"
      );
    }

    function renderRestartToggle(state, stepId) {
      var checked = true;
      if (state && state.restartChoices && state.restartChoices.hasOwnProperty(stepId)) {
        checked = !!state.restartChoices[stepId];
      }
      return "<label class='choiceRow'><input id='restartNowToggle' type='checkbox' " +
        (checked ? "checked='checked'" : "") +
        " /><span class='choiceText'>Restart now after this step</span></label>";
    }

    function renderBitLockerStep(profile) {
      var restartSummary = (profile && profile.rebootAfterDisableBitLocker)
        ? "Saved profile action: restart immediately after BitLocker is disabled."
        : "Saved profile action: continue to application installs without an immediate restart.";
      return "<div class='choiceRow'><span class='choiceText'>" +
        htmlEncode(restartSummary) +
        "</span></div>";
    }

    function renderAppProgress(appProgress, show) {
      var card = document.getElementById("appProgressCard");
      if (!show) {
        card.style.display = "none";
        return;
      }

      card.style.display = "block";
      var completed = appProgress && appProgress.completedCount ? appProgress.completedCount : 0;
      var total = appProgress && appProgress.totalCount ? appProgress.totalCount : 0;
      var currentItem = appProgress && appProgress.currentItem ? appProgress.currentItem : "Waiting to start";
      var width = total > 0 ? Math.round((completed / total) * 100) : 0;
      setTextIfChanged("appProgressCurrent", currentItem);
      setTextIfChanged("appProgressCount", completed + " / " + total);
      document.getElementById("appProgressFill").style.width = width + "%";
    }

    function renderProvisioningView(profile, state, status, appProgress) {
      var rendered = false;
      var profileSignature = signatureFor(profile);
      var statusSignature = signatureFor(status);
      var stateSignature = signatureFor(state);
      var appProgressSignature = signatureFor(appProgress);
      var mainSignature = profileSignature + "::" + stateSignature + "::" + statusSignature + "::" + appProgressSignature;

      if (lastRenderSignatures.profile !== profileSignature) {
        renderSettings(profile);
        lastRenderSignatures.profile = profileSignature;
        rendered = true;
      }
      if (lastRenderSignatures.status !== statusSignature) {
        renderTasks(status);
        lastRenderSignatures.status = statusSignature;
        rendered = true;
      }
      if (lastRenderSignatures.main !== mainSignature) {
        renderMain(profile, state, status, appProgress);
        lastRenderSignatures.main = mainSignature;
        rendered = true;
      }

      return rendered;
    }

    function submitCurrentStep() {
      var now = new Date().getTime();
      if ((now - lastCommandAt) < 1000) { return; }
      lastCommandAt = now;

      var state = readJsonDocument(statePath, "state") || {};
      var profile = readJsonDocument(profilePath, "profile") || {};
      if (state.inProgress) { return; }

      var command = {
        action: "submit",
        stepId: state.currentStepId || "",
        computerName: "",
        restartNow: true,
        submittedAtUtc: new Date().toISOString()
      };

      var computerNameInput = document.getElementById("computerNameInput");
      if (computerNameInput) {
        command.computerName = computerNameInput.value || "";
      }

      var restartToggle = document.getElementById("restartNowToggle");
      if (command.stepId === "bitLocker") {
        command.restartNow = !!profile.rebootAfterDisableBitLocker;
      } else if (restartToggle) {
        command.restartNow = !!restartToggle.checked;
      }

      writeFileText(commandPath, JSON.stringify(command, null, 2));
      appendShellLog("Submitting provisioning step " + command.stepId + " restartNow=" + command.restartNow + ".");

      try {
        var shell = new ActiveXObject("WScript.Shell");
        shell.Run("powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"" + controllerPath + "\" -Action ProcessCommand", 0, false);
      } catch (error) {
        appendShellLog("Controller launch failed: " + errorMessage(error));
        statusBanner("error", "Unable to start the provisioning backend.");
      }
    }

    function scheduleNextTick(delayMs) {
      if (tickTimer !== null) {
        window.clearTimeout(tickTimer);
      }
      tickTimer = window.setTimeout(runTick, delayMs);
    }

    function shouldLogTick(durationMs) {
      var now = new Date().getTime();
      if (!lastTickLogAt || durationMs >= 1000 || (now - lastTickLogAt) >= 30000) {
        lastTickLogAt = now;
        return true;
      }
      return false;
    }

    function runTick() {
      if (tickInFlight) {
        appendShellLog("Skipped overlapping provisioning tick.");
        scheduleNextTick(tickIntervalMs);
        return;
      }

      tickInFlight = true;
      tickTimer = null;
      var tickStartedAt = new Date();
      heartbeatState.inTick = true;
      heartbeatState.lastTickUtc = tickStartedAt.toISOString();
      writeHeartbeat();

      try {
        var profile = readJsonDocument(profilePath, "profile");
        var state = readJsonDocument(statePath, "state");
        var status = readJsonDocument(statusPath, "status");
        var appProgress = readJsonDocument(appProgressPath, "appProgress");
        var rendered = renderProvisioningView(profile, state, status, appProgress);
        heartbeatState.tickDurationMs = new Date().getTime() - tickStartedAt.getTime();
        heartbeatState.lastError = "";
        if (rendered) {
          heartbeatState.lastRenderUtc = new Date().toISOString();
        }
        if (shouldLogTick(heartbeatState.tickDurationMs)) {
          appendShellLog("Tick succeeded in " + heartbeatState.tickDurationMs + "ms.");
        }
      } catch (error) {
        heartbeatState.tickDurationMs = new Date().getTime() - tickStartedAt.getTime();
        heartbeatState.lastError = errorMessage(error);
        appendShellLog("tick failed: " + heartbeatState.lastError);
        statusBanner("error", "Provisioning UI hit a script error. See the BitOSDT shell log.");
      } finally {
        heartbeatState.inTick = false;
        writeHeartbeat();
        tickInFlight = false;
        scheduleNextTick(tickIntervalMs);
      }
    }

    function applyLayoutMode() {
      var shell = document.getElementById("shell");
      if (!shell) { return; }
      var width = 0;
      var height = 0;
      try {
        width = document.documentElement && document.documentElement.clientWidth ? document.documentElement.clientWidth : 0;
        height = document.documentElement && document.documentElement.clientHeight ? document.documentElement.clientHeight : 0;
      } catch (error) {
      }
      if (!width && document.body) { width = document.body.clientWidth; }
      if (!height && document.body) { height = document.body.clientHeight; }
      if (!width && screen) { width = screen.availWidth; }
      if (!height && screen) { height = screen.availHeight; }

      shell.className = (width < 1280 || height < 820) ? "compact" : "";
    }

    function fitWindowToScreen() {
      try {
        window.moveTo(0, 0);
        window.resizeTo(screen.availWidth, screen.availHeight);
      } catch (error) {
      }
    }

    window.onerror = function(message, source, lineno) {
      heartbeatState.lastError = "line " + lineno + ": " + message;
      writeHeartbeat();
      appendShellLog("window.onerror: " + source + " line " + lineno + ": " + message);
      statusBanner("error", "Provisioning UI hit a script error. See the BitOSDT shell log.");
      return true;
    };

    window.onload = function() {
      appendShellLog("window.onload fired.");
      fitWindowToScreen();
      applyLayoutMode();
      writeHeartbeat();
      scheduleNextTick(80);
    };

    window.onresize = function() {
      applyLayoutMode();
    };
  </script>
</body>
</html>
"##
    .to_string();

    html = html.replace("__PROFILE_PATH__", &js_string_literal(profile_path));
    html = html.replace("__STATE_PATH__", &js_string_literal(state_path));
    html = html.replace("__STATUS_PATH__", &js_string_literal(status_path));
    html = html.replace(
        "__APP_PROGRESS_PATH__",
        &js_string_literal(app_progress_path),
    );
    html = html.replace("__COMMAND_PATH__", &js_string_literal(command_path));
    html = html.replace("__CONTROLLER_PATH__", &js_string_literal(controller_path));
    html = html.replace("__HEARTBEAT_PATH__", &js_string_literal(heartbeat_path));
    html = html.replace("__SHELL_LOG_PATH__", &js_string_literal(shell_log_path));
    html
}

pub fn generate_credential_prompt_hta(mode: &str) -> String {
    let title = if mode.eq_ignore_ascii_case("domain") {
        "Domain Credentials"
    } else {
        "UNC Credentials"
    };
    let username_label = if mode.eq_ignore_ascii_case("domain") {
        r"Domain\\Username"
    } else {
        "Username"
    };

    format!(
        r##"<html>
<head>
  <title>{title}</title>
  <HTA:APPLICATION
    APPLICATIONNAME="BitOSDTCredentialPrompt"
    BORDER="thin"
    CAPTION="yes"
    SHOWINTASKBAR="yes"
    SINGLEINSTANCE="yes"
    SYSMENU="no"
    SCROLL="no"
    WINDOWSTATE="normal"
  />
  <meta http-equiv="X-UA-Compatible" content="IE=edge" />
  <style>
    html, body {{
      margin: 0;
      padding: 0;
      height: 100%;
      width: 100%;
      font-family: "Segoe UI", Tahoma, Arial, sans-serif;
      background: #f0f4f8;
      color: #1e293b;
    }}
    .credential-container {{
      max-width: 420px;
      margin: 60px auto;
      padding: 32px;
      background: #ffffff;
      border-radius: 12px;
      box-shadow: 0 4px 24px rgba(0,0,0,0.12);
    }}
    .credential-container h1 {{
      font-size: 20px;
      margin: 0 0 8px 0;
    }}
    .credential-container p {{
      font-size: 13px;
      color: #64748b;
      margin: 0 0 20px 0;
    }}
    .field {{
      margin-bottom: 14px;
    }}
    .field label {{
      display: block;
      font-size: 13px;
      font-weight: 600;
      margin-bottom: 4px;
    }}
    .field input {{
      width: 100%;
      box-sizing: border-box;
      padding: 8px 12px;
      font-size: 14px;
      border: 1px solid #cbd5e1;
      border-radius: 6px;
    }}
    .actions {{
      margin-top: 20px;
      display: flex;
      gap: 10px;
      justify-content: flex-end;
    }}
    .btn {{
      padding: 8px 20px;
      font-size: 14px;
      border-radius: 6px;
      border: 1px solid #cbd5e1;
      cursor: pointer;
    }}
    .btn-primary {{
      background: #2563eb;
      color: #ffffff;
      border-color: #2563eb;
    }}
    .btn-secondary {{
      background: #ffffff;
      color: #475569;
    }}
    .error-text {{
      color: #dc2626;
      font-size: 12px;
      margin-top: 4px;
      display: none;
    }}
  </style>
</head>
<body>
  <div class="credential-container">
    <h1>{title}</h1>
    <p>Enter the credentials required to continue the deployment.</p>
    <div class="field">
      <label for="username">{username_label}</label>
      <input type="text" id="username" autofocus />
    </div>
    <div class="field">
      <label for="password">Password</label>
      <input type="password" id="password" />
    </div>
    <div id="error" class="error-text">Please enter both a username and password.</div>
    <div class="actions">
      <button class="btn btn-secondary" id="cancelBtn" onclick="handleCancel()">Cancel</button>
      <button class="btn btn-primary" id="submitBtn" onclick="handleSubmit()">Continue</button>
    </div>
  </div>
  <script>
    var mode = "{mode}";
    var resultFile = "X:\\BitOSDT\\State\\credential-prompt-result.json";

    function jsonEscape(value) {{
      return String(value)
        .replace(/\\/g, '\\\\')
        .replace(/"/g, '\\"')
        .replace(/\r/g, '\\r')
        .replace(/\n/g, '\\n')
        .replace(/\t/g, '\\t');
    }}

    function handleSubmit() {{
      var username = document.getElementById('username').value;
      var password = document.getElementById('password').value;
      if (!username || !password) {{
        document.getElementById('error').style.display = 'block';
        return;
      }}
      var fso = new ActiveXObject("Scripting.FileSystemObject");
      var ts = fso.CreateTextFile(resultFile, true);
      ts.WriteLine('{{"status":"ok","mode":"' + jsonEscape(mode) + '","username":"' + jsonEscape(username) + '","password":"' + jsonEscape(password) + '"}}');
      ts.Close();
      window.close();
    }}

    function handleCancel() {{
      var fso = new ActiveXObject("Scripting.FileSystemObject");
      var ts = fso.CreateTextFile(resultFile, true);
      ts.WriteLine('{{"status":"cancelled","mode":"' + mode + '"}}');
      ts.Close();
      window.close();
    }}
  </script>
</body>
</html>"##
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainCredentialPromptResult {
    pub domain: String,
    pub ou_path: Option<String>,
    pub username: String,
    pub password: String,
}

pub fn generate_domain_credential_prompt_hta(
    default_domain: Option<&str>,
    default_ou_path: Option<&str>,
) -> String {
    let default_domain = js_string_literal(default_domain.unwrap_or(""));
    let default_ou_path = js_string_literal(default_ou_path.unwrap_or(""));

    format!(
        r##"<html>
<head>
  <title>Domain Credentials</title>
  <HTA:APPLICATION
    APPLICATIONNAME="BitOSDTDomainCredentialPrompt"
    BORDER="thin"
    CAPTION="yes"
    SHOWINTASKBAR="yes"
    SINGLEINSTANCE="yes"
    SYSMENU="no"
    SCROLL="no"
    WINDOWSTATE="normal"
  />
  <meta http-equiv="X-UA-Compatible" content="IE=edge" />
  <style>
    html, body {{
      margin: 0;
      padding: 0;
      height: 100%;
      width: 100%;
      font-family: "Segoe UI", Tahoma, Arial, sans-serif;
      background: #f0f4f8;
      color: #1e293b;
    }}
    .credential-container {{
      max-width: 440px;
      margin: 40px auto;
      padding: 32px;
      background: #ffffff;
      border-radius: 12px;
      box-shadow: 0 4px 24px rgba(0,0,0,0.12);
    }}
    .credential-container h1 {{
      font-size: 20px;
      margin: 0 0 8px 0;
    }}
    .credential-container p {{
      font-size: 13px;
      color: #64748b;
      margin: 0 0 20px 0;
    }}
    .field {{
      margin-bottom: 14px;
    }}
    .field label {{
      display: block;
      font-size: 13px;
      font-weight: 600;
      margin-bottom: 4px;
    }}
    .field input {{
      width: 100%;
      box-sizing: border-box;
      padding: 8px 12px;
      font-size: 14px;
      border: 1px solid #cbd5e1;
      border-radius: 6px;
    }}
    .actions {{
      margin-top: 20px;
      display: flex;
      gap: 10px;
      justify-content: flex-end;
    }}
    .btn {{
      padding: 8px 20px;
      font-size: 14px;
      border-radius: 6px;
      border: 1px solid #cbd5e1;
      cursor: pointer;
    }}
    .btn-primary {{
      background: #2563eb;
      color: #ffffff;
      border-color: #2563eb;
    }}
    .btn-secondary {{
      background: #ffffff;
      color: #475569;
    }}
    .error-text {{
      color: #dc2626;
      font-size: 12px;
      margin-top: 4px;
      display: none;
    }}
  </style>
</head>
<body>
  <div class="credential-container">
    <h1>Domain Credentials</h1>
    <p>Enter the domain details required to continue the deployment.</p>
    <div class="field">
      <label for="domain">Domain</label>
      <input type="text" id="domain" />
    </div>
    <div class="field">
      <label for="ouPath">OU Path (Optional)</label>
      <input type="text" id="ouPath" />
    </div>
    <div class="field">
      <label for="username">Domain\Username</label>
      <input type="text" id="username" />
    </div>
    <div class="field">
      <label for="password">Password</label>
      <input type="password" id="password" />
    </div>
    <div id="error" class="error-text">Please enter a domain, username, and password.</div>
    <div class="actions">
      <button class="btn btn-secondary" id="cancelBtn" onclick="handleCancel()">Cancel</button>
      <button class="btn btn-primary" id="submitBtn" onclick="handleSubmit()">Continue</button>
    </div>
  </div>
  <script>
    var resultFile = "X:\\BitOSDT\\State\\credential-prompt-result.json";
    var defaultDomain = {default_domain};
    var defaultOuPath = {default_ou_path};

    function jsonEscape(value) {{
      return String(value)
        .replace(/\\/g, '\\\\')
        .replace(/"/g, '\\"')
        .replace(/\r/g, '\\r')
        .replace(/\n/g, '\\n')
        .replace(/\t/g, '\\t');
    }}

    function initializeForm() {{
      document.getElementById('domain').value = defaultDomain;
      document.getElementById('ouPath').value = defaultOuPath;
      if (defaultDomain) {{
        document.getElementById('username').focus();
      }} else {{
        document.getElementById('domain').focus();
      }}
    }}

    function handleSubmit() {{
      var domain = document.getElementById('domain').value.replace(/^\s+|\s+$/g, '');
      var ouPath = document.getElementById('ouPath').value.replace(/^\s+|\s+$/g, '');
      var username = document.getElementById('username').value.replace(/^\s+|\s+$/g, '');
      var password = document.getElementById('password').value;
      if (!domain || !username || !password) {{
        document.getElementById('error').style.display = 'block';
        return;
      }}
      var fso = new ActiveXObject("Scripting.FileSystemObject");
      var ts = fso.CreateTextFile(resultFile, true);
      ts.WriteLine('{{"status":"ok","mode":"domain","domain":"' + jsonEscape(domain) + '","ou_path":"' + jsonEscape(ouPath) + '","username":"' + jsonEscape(username) + '","password":"' + jsonEscape(password) + '"}}');
      ts.Close();
      window.close();
    }}

    function handleCancel() {{
      var fso = new ActiveXObject("Scripting.FileSystemObject");
      var ts = fso.CreateTextFile(resultFile, true);
      ts.WriteLine('{{"status":"cancelled","mode":"domain"}}');
      ts.Close();
      window.close();
    }}

    window.onload = initializeForm;
  </script>
</body>
</html>"##
    )
}

#[cfg(target_os = "windows")]
fn launch_prompt_and_read_result(hta_content: &str) -> Result<serde_json::Value, String> {
    let hta_path = r"X:\BitOSDT\Scripts\CredentialPrompt.hta";
    let result_path = r"X:\BitOSDT\State\credential-prompt-result.json";

    use std::io::Write;
    if let Some(parent) = std::path::Path::new(hta_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Some(parent) = std::path::Path::new(result_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut file =
        std::fs::File::create(hta_path).map_err(|e| format!("Failed to create HTA file: {}", e))?;
    file.write_all(hta_content.as_bytes())
        .map_err(|e| format!("Failed to write HTA file: {}", e))?;
    drop(file);

    let _ = std::fs::remove_file(result_path);

    let _output = std::process::Command::new("mshta.exe")
        .arg(hta_path)
        .output()
        .map_err(|e| format!("Failed to launch credential prompt HTA: {}", e))?;

    let result_json = std::fs::read_to_string(result_path)
        .map_err(|e| format!("Failed to read credential prompt result: {}", e))?;

    let _ = std::fs::remove_file(result_path);

    serde_json::from_str(&result_json)
        .map_err(|e| format!("Failed to parse credential prompt result: {}", e))
}

#[cfg(target_os = "windows")]
pub fn launch_credential_prompt(mode: &str) -> Result<(String, String), String> {
    let hta_content = generate_credential_prompt_hta(mode);
    let parsed = launch_prompt_and_read_result(&hta_content)?;

    let status = parsed
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("cancelled");

    if status != "ok" {
        return Err("Credential prompt was cancelled by the user.".to_string());
    }

    let username = parsed
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let password = parsed
        .get("password")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok((username, password))
}

#[cfg(target_os = "windows")]
pub fn launch_domain_credential_prompt(
    default_domain: Option<&str>,
    default_ou_path: Option<&str>,
) -> Result<DomainCredentialPromptResult, String> {
    let parsed = launch_prompt_and_read_result(&generate_domain_credential_prompt_hta(
        default_domain,
        default_ou_path,
    ))?;

    let status = parsed
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or("cancelled");

    if status != "ok" {
        return Err("Credential prompt was cancelled by the user.".to_string());
    }

    let domain = parsed
        .get("domain")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let username = parsed
        .get("username")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let password = parsed
        .get("password")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let ou_path = parsed
        .get("ou_path")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());

    if domain.is_empty() || username.is_empty() || password.is_empty() {
        return Err("Credential prompt returned incomplete domain credentials.".to_string());
    }

    Ok(DomainCredentialPromptResult {
        domain,
        ou_path,
        username,
        password,
    })
}

#[cfg(not(target_os = "windows"))]
pub fn launch_credential_prompt(mode: &str) -> Result<(String, String), String> {
    Err(format!(
        "Credential prompt is only available on Windows (WinPE). Requested mode: {}",
        mode
    ))
}

#[cfg(not(target_os = "windows"))]
pub fn launch_domain_credential_prompt(
    _default_domain: Option<&str>,
    _default_ou_path: Option<&str>,
) -> Result<DomainCredentialPromptResult, String> {
    Err(
        "Credential prompt is only available on Windows (WinPE). Requested mode: domain"
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provisioning_hta_contains_expected_kiosk_hooks() {
        let html = generate_provisioning_hta(
            r"C:\ProgramData\BitOSDT\ProvisioningUi\profile.json",
            r"C:\ProgramData\BitOSDT\ProvisioningUi\ui-state.json",
            r"C:\ProgramData\BitOSDT\ProvisioningUi\task-status.json",
            r"C:\ProgramData\BitOSDT\ProvisioningUi\app-progress.json",
            r"C:\ProgramData\BitOSDT\ProvisioningUi\command.json",
            r"C:\BitOSDT\Scripts\Start-BitOSDTOrchestrator.ps1",
            r"C:\ProgramData\BitOSDT\ProvisioningUi\ui-heartbeat.json",
            r"C:\BitOSDT\Logs\provisioning-shell.log",
        );
        assert!(html.contains("BitOSDT Provisioning"));
        assert!(html.contains("id=\"computerNameInput\"") || html.contains("computerNameInput"));
        assert!(html.contains("Restart now after this step"));
        assert!(html.contains("state.currentStepId === \"bitLocker\""));
        assert!(
            html.contains("Saved profile action: restart immediately after BitLocker is disabled.")
        );
        assert!(html.contains("var tickIntervalMs = 1200;"));
        assert!(html.contains("var heartbeatPath = \"C:\\\\ProgramData\\\\BitOSDT\\\\ProvisioningUi\\\\ui-heartbeat.json\";"));
        assert!(html.contains("var tickInFlight = false;"));
        assert!(html.contains("var lastTickLogAt = 0;"));
        assert!(html.contains("window.setTimeout(runTick, delayMs);"));
        assert!(html.contains("function shouldLogTick(durationMs) {"));
        assert!(html.contains("Skipped overlapping provisioning tick."));
        assert!(html.contains("window.resizeTo(screen.availWidth, screen.availHeight);"));
        assert!(html.contains(
            r#"var statusPath = "C:\\ProgramData\\BitOSDT\\ProvisioningUi\\task-status.json";"#
        ));
        assert!(html.contains("function replaceHtmlIfChanged(element, html) {"));
        assert!(html.contains("function readJsonDocument(path, key) {"));
        assert!(html.contains("Using cached \" + key + \" data after JSON parse issue"));
        assert!(
            html.contains("writeFileText(heartbeatPath, JSON.stringify(heartbeatState, null, 2));")
        );
        assert!(html.contains("focusComputerNameInput(true);"));
        assert!(html.contains("taskStateText"));
        assert!(html.contains(".taskState.reboot_pending"));
        assert!(html.contains("shell.Run(\"powershell.exe -NoProfile -ExecutionPolicy Bypass -File \\\"\" + controllerPath + \"\\\" -Action ProcessCommand\""));
        assert!(html.contains("var fileReadCache = {};"));
        assert!(html.contains(
            "if (cache && !cache.missing && cache.stamp === stamp && cache.size === size) {"
        ));
        assert!(!html.contains("new ActiveXObject(\"ADODB.Stream\")"));
        assert!(html.contains("appendShellLog(\"window.onload fired.\");"));
        assert!(html.contains(
            "appendShellLog(\"Tick succeeded in \" + heartbeatState.tickDurationMs + \"ms.\");"
        ));
        assert!(html.contains("scheduleNextTick(80);"));
        assert!(html.contains(r#"<div id="actionButtonRow"><button id="nextButton" onclick="submitCurrentStep()">Next</button></div>"#));
        let footer_index = html.find(r#"<div id="footerNote">If a step needs a reboot, BitOSDT resumes here automatically.</div>"#).unwrap();
        let button_row_index = html.find(r#"<div id="actionButtonRow"><button id="nextButton" onclick="submitCurrentStep()">Next</button></div>"#).unwrap();
        assert!(footer_index < button_row_index);
    }

    #[test]
    fn provisioning_kiosk_helper_retargets_title_and_log_path() {
        let ps1 = generate_provisioning_kiosk_helper_ps1(
            r"C:\BitOSDT\Logs\provisioning-shell.log",
            "BitOSDT Provisioning",
        );
        assert!(ps1.contains(r#"C:\BitOSDT\Logs\provisioning-shell.log"#));
        assert!(ps1.contains(r#"$WindowTitle = "BitOSDT Provisioning""#));
        assert!(!ps1.contains(r#"X:\BitOSDT\Logs\shell-launch.log"#));
    }

    #[test]
    fn domain_credential_prompt_hta_includes_domain_fields_and_defaults() {
        let html = generate_domain_credential_prompt_hta(
            Some("contoso.local"),
            Some("OU=Devices,DC=contoso,DC=local"),
        );

        assert!(html.contains("Domain Credentials"));
        assert!(html.contains("id=\"domain\""));
        assert!(html.contains("id=\"ouPath\""));
        assert!(html.contains("Domain\\Username"));
        assert!(html.contains("contoso.local"));
        assert!(html.contains("OU=Devices,DC=contoso,DC=local"));
        assert!(html.contains("\"ou_path\""));
    }

    #[test]
    fn credential_prompt_hta_escapes_json_sensitive_characters() {
        let html = generate_credential_prompt_hta("UNC");

        assert!(html.contains("function jsonEscape(value)"));
        assert!(html.contains(r#".replace(/\\/g, '\\\\')"#));
        assert!(html.contains(r#".replace(/"/g, '\\"')"#));
        assert!(html.contains(r#".replace(/\r/g, '\\r')"#));
        assert!(html.contains(r#".replace(/\n/g, '\\n')"#));
        assert!(html.contains(r#".replace(/\t/g, '\\t')"#));
        assert!(html.contains(r#""mode":"' + jsonEscape(mode)"#));
        assert!(html.contains(r#""username":"' + jsonEscape(username)"#));
        assert!(html.contains(r#""password":"' + jsonEscape(password)"#));
        assert!(!html.contains(r#"username.replace(/"/g, '\\\\"')"#));
    }
}
