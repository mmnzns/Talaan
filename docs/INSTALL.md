# Installing Talaan

Talaan is a free meeting recorder that stays on your computer. It records both sides of any call (your microphone and the call's audio), then writes a transcript and a summary with decisions and action items. No bot joins your meeting. Speech recognition and summaries run on your computer, with Wi-Fi off too.

It works on Windows and on Mac.

- [Windows](#windows)
- [Mac](#mac)
- [Things to know on both](#things-to-know-on-both)
- [Troubleshooting](#troubleshooting)
- [Removing Talaan](#removing-talaan)

> **Mac users: macOS blocks Talaan twice. That's expected.**
>
> Once when you open the `.dmg`, and once the first time you open Talaan. Each time:
>
> 1. Click **Done**. Never **Move to Trash**.
> 2. Open **System Settings › Privacy & Security**, scroll down and click **Open Anyway**.
> 3. Enter your Mac password, then click **Open Anyway** again.
>
> Why: Talaan is free and made by one person. It isn't signed with a paid Apple or Microsoft certificate, so macOS and Windows can't vouch for it and ask you first. Windows asks once too (see [Install](#install)). Everything is built from the public source code at https://github.com/mmnzns/Talaan, which anyone can read.

---

## Windows

### Requirements

- Windows 11, or Windows 10 version 2004 or later.
- A 64-bit Intel or AMD PC (x64).
- About 3 to 4 GB of free space for the speech and summary models, plus room for your meetings.
- An internet connection the first time, to download the models. After that Talaan works offline.

### Download

Go to **https://github.com/mmnzns/Talaan/releases/latest** and download the Windows file.

| File | What it is |
|---|---|
| `Talaan_1.0.0_x64-setup.exe` | The Windows installer. This is the one you want. |
| `Talaan_1.0.0_aarch64.dmg` | The Mac version. Skip it. |
| `latest.json`, `.sig`, `Talaan_aarch64.app.tar.gz` | Used by Talaan's built-in updater. Skip them. |

The version number in the file name changes with each release.

Edge or Chrome may warn that the file isn't commonly downloaded. In Edge: **...** › **Keep** › **Show more** › **Keep anyway**.

### Install

1. Double-click `Talaan_..._x64-setup.exe`.
2. Windows may show **Windows protected your PC**. Click **More info**, then **Run anyway**. This happens once. The publisher shows as Unknown publisher. That's expected.
3. Follow the installer. It installs for your Windows account only and doesn't ask for an administrator password. If your PC doesn't have Microsoft Edge WebView2 (rare), the installer downloads it.
4. Leave **Run Talaan** ticked on the last page. Talaan opens at the end.

### First setup

Talaan walks you through seven short steps the first time it opens.

1. **Welcome.** Click **Get started**.
2. **Audio access** ("Allow two kinds of audio"):
   - **Microphone** records what you say, labeled "Me". Click **Allow**.
   - **System audio** records the other people on the call. On Windows it needs no permission.
3. **Speech model** ("Choose a speech model"). Pick one, then click **Download** when you're ready (nothing downloads until you do; you can also skip it and download later in Settings). See [Which speech model](#which-speech-model) below. On Windows, **Parakeet** is much faster.
4. **Summaries** ("How summaries are written"). **On this computer** is the default: built in and offline, a one-time download that starts only when you click **Download**. **Your own key** (Claude, OpenAI, Gemini, Groq, Cerebras, Mistral, OpenRouter or any OpenAI-compatible service) sends transcript text only, and you set it up later in Settings.
5. **Call apps** ("When should Talaan record?"). By default Talaan asks every time a call app uses your mic. Turn on **Always record** for apps you always want recorded.
6. **Extras** ("Optional extras"). Calendar, browser call detection and Notion. All are off by default and can be turned on later in Settings. On Windows the calendar is set up in Settings (see [Calendar on Windows](#calendar-on-windows)).
7. **Done.** Click **Open Talaan**.

### Where Talaan lives

Talaan keeps running in the **system tray**, at the bottom right of the screen, next to the clock. Its icon is a gold "ta". If you don't see it, click the small **^** arrow to show hidden icons, and drag the Talaan icon onto the taskbar so it stays visible.

- Click the icon to start a recording, see recent meetings, open Talaan or **Quit**.
- While recording, hover over the icon to see the timer.
- Closing the main window doesn't quit Talaan. It keeps listening for calls in the tray.
- **Ctrl+,** opens Settings.
- When a summary is ready, a Windows notification appears. Click it to open the meeting.

### Updates

Talaan checks GitHub for a new version when it starts (you can turn this off in **Settings › General › Check for updates automatically**). When one is ready, an **update ready** chip appears in the toolbar, and **Settings › General › Updates** shows **Update now**.

Click **Update now**. A small installer window shows its progress, then Talaan opens again by itself. Your meetings and settings stay. Nothing is downloaded until you click. Updates are signed with Talaan's own key, so Talaan only installs updates that really come from this project.

---

## Mac

### Requirements

- macOS 14.4 (Sonoma) or later.
- A Mac with Apple silicon (M1 or newer). Intel Macs aren't supported.
- About 3 to 4 GB of free space for the speech and summary models, plus room for your meetings.
- An internet connection the first time, to download the models. After that Talaan works offline.

### Download

Go to **https://github.com/mmnzns/Talaan/releases/latest** and download `Talaan_1.0.0_aarch64.dmg` (the version number changes with each release). Ignore the other files: the `.exe` is for Windows, and `latest.json`, `.sig` and `.tar.gz` are for the updater.

### Install

1. Double-click the `.dmg`. macOS says it can't check it for malicious software.
   - Click **Done** (not **Move to Trash**).
   - Open **System Settings › Privacy & Security**, scroll down and click **Open Anyway**.
   - Enter your password, then click **Open Anyway** again.
2. In the window that opens, drag **Talaan** into **Applications**.
3. Open Talaan from Applications. macOS blocks it once more.
   - Click **Done** again.
   - **System Settings › Privacy & Security › Open Anyway**, your password, then **Open Anyway**.

You only do this once. Updates open normally.

<details>
<summary>Prefer Terminal? Skip both blocks with one command each</summary>

This removes the "downloaded from the internet" mark that makes macOS block the file. Run the first line before opening the `.dmg` (adjust the file name if yours differs), and the second after dragging Talaan to Applications.

```bash
xattr -d com.apple.quarantine ~/Downloads/Talaan_1.0.0_aarch64.dmg
xattr -d com.apple.quarantine /Applications/Talaan.app
```

If it says "No such xattr", the mark is already gone and you can open Talaan normally.

</details>

### First setup

Talaan walks you through seven short steps. macOS asks for permission along the way. Click **Allow** each time.

1. **Welcome.** Click **Get started**.
2. **Audio access** ("Allow two kinds of audio"):
   - **Microphone** records what you say, labeled "Me". Click **Allow**, then **Allow** in the macOS prompt.
   - **System audio** records the other people on the call. macOS may call this "Screen & System Audio Recording". Talaan only uses the audio. Click **Allow**. If you say no, Talaan records your side only until you turn it on in **System Settings › Privacy & Security**.
3. **Speech model** ("Choose a speech model"). **Whisper** is recommended on the Mac. See [Which speech model](#which-speech-model).
4. **Summaries** ("How summaries are written"). **On this computer** is the default: built in and offline, a one-time download. **Your own key** sends transcript text only, set up later in Settings.
5. **Call apps** ("When should Talaan record?"). Talaan asks by default. Turn on **Always record** for apps you always want recorded.
6. **Extras** ("Optional extras"), all off by default:
   - **Calendar**: macOS asks for access to your calendars. Talaan reads the event during a recording to name the meeting and suggest who spoke. It never changes your calendar.
   - **Browser call detection**: the first time a browser call happens, macOS asks to let Talaan control that browser (Chrome, Safari, Arc...). Talaan only reads tab addresses, to tell a Google Meet tab from a YouTube tab.
   - **Notion**: set up later in **Settings › Connections**.
7. **Done.** Click **Open Talaan**.

The first time a summary finishes, macOS asks whether Talaan can send notifications. Allow it to get "Summary ready" alerts.

### Where Talaan lives

Talaan lives in the **menu bar** at the top of the screen, as a small "ta" icon. Click it to start a recording, see recent meetings, open Talaan or **Quit**. While recording, the timer shows next to the icon. Closing the main window doesn't quit Talaan; click the Dock icon to bring it back. **Cmd+,** opens Settings.

### Updates

Same as on Windows: an **update ready** chip in the toolbar, or **Settings › General › Updates › Update now**. Talaan installs the update and restarts by itself. You won't see the security blocks again, and your permissions, meetings and settings stay.

---

## Things to know on both

### Call detection

Talaan notices when a call app starts using your microphone: Zoom, Microsoft Teams, Slack, Webex, Discord, WhatsApp, FaceTime (Mac) and browser calls. It then shows **Record this call?** with **Record**, **Not now** and **Always for (app)**.

- Set each app to **Ask**, **Always** or **Never** in **Settings › Call apps**. Zoom starts on **Always** and FaceTime (Mac) on **Never**; the others, browser calls included, start on **Ask**. FaceTime and the Mac-only dictation apps aren't listed on Windows.
- **Always** still shows a banner, with **Copy notice** (a message to paste in the call chat) and **Stop**.
- When the call app lets go of the mic, Talaan says **Call ended. Stop recording?** and stops by itself 30 seconds later unless you click **Keep recording**.
- Dictation and voice apps (Parlato, Wispr Flow, Windows voice typing, NVIDIA Broadcast, Krisp and others) never trigger it. The list is in **Settings › Call apps › Ignored apps**.
- To turn detection off entirely: **Settings › General › Detect calls**.

### Browser calls

With **Settings › Connections › Browser call detection** on, Talaan can tell a Google Meet, Zoom web or Teams web tab from other sites using the mic, so **Browser calls** can be set to **Always**.

- **Mac**: works with Chrome, Safari, Arc and other browsers after you allow each one once.
- **Windows**: in Chrome or Edge (and other Chromium browsers), Talaan reads the address of the tab in front and the names of the other tabs, only while the browser is using your microphone. A call in a background tab is recognized by its tab name (for example "Meet - abc-defg-hij" or "… | Microsoft Teams"). If Talaan still can't tell, it asks instead of recording automatically. Firefox calls are always asked about.

### Calendar on Windows

Windows has no calendar Talaan can read directly, so you give it your calendar's private iCal address instead. Talaan only downloads from that address; nothing about your meetings is sent.

1. Copy your calendar's address:
   - **Google Calendar**: on the web, **Settings**, click your calendar under "Settings for my calendars", then copy **Secret address in iCal format**.
   - **Outlook / Microsoft 365**: on Outlook on the web, **Settings › Calendar › Shared calendars › Publish a calendar**, pick your calendar, **Publish**, then copy the **ICS** link.
   - **iCloud**: share the calendar as a public calendar and copy its link.
2. In Talaan, open **Settings › Connections › Calendar** and paste the address.
3. Click **Test and connect**. Talaan shows how many events it found in the next 7 days.

Keep this address private: anyone who has it can read your calendar. On the Mac, Talaan reads the Calendar app directly and you don't need an address.

### Where your meetings are saved

Meetings are plain files (transcript, summary, audio) in Talaan's data folder:

- **Windows**: `%APPDATA%\Talaan` (for example `C:\Users\you\AppData\Roaming\Talaan`)
- **Mac**: `~/Library/Application Support/Talaan`

**Settings › Privacy › Meetings folder** shows the path and size, with **Open folder** and **Export all** (every meeting as Markdown, to a folder you choose). Audio is kept 7 days by default (change it in **Settings › Privacy › Keep audio for**); pin a meeting to keep its audio.

### Models download once

Speech, summary, speaker and search models download only when you click Download (in setup or Settings), then stay on your computer. They live in the same data folder. You can switch or delete them in **Settings › Speech models** and **Settings › Summary AI**. If another speech app already downloaded the same model, Talaan can reuse it read-only (**Reuse models already on this computer**).

#### Which speech model

| Model | Best for | Download |
|---|---|---|
| **Whisper Large Turbo** | Most accurate, 99 languages including Filipino. Recommended on the Mac. | 575 MB |
| **Parakeet** | Fastest, 25 European languages. Best choice on most Windows PCs. | 671 MB |
| **Whisper Small** | Lighter, for older computers. Lower accuracy. | 488 MB |

Setup shows these three. **Show all models** (or Settings › Speech models) lists the rest:

| Model | Best for | Download |
|---|---|---|
| **Whisper Large v3** / **Large v2** | Whisper's largest, very accurate, slower than Turbo. 99 languages. | 1.1 GB each |
| **Whisper Medium** | Between Small and Large. 99 languages. | 1.5 GB |
| **Whisper Base** / **Tiny** | Very small and fast, rough on names and accents. | 148 / 78 MB |
| **Parakeet Ultra** | Parakeet v3 further trained by Moondream, more accurate. 25 European languages. | 668 MB |
| **Parakeet v2** | English only, fast and very accurate in English. | 662 MB |
| **Parakeet Unified** | English only, NVIDIA's newer English model. | 664 MB |
| **Nemotron 3.5** | NVIDIA's newest, 35 languages. | 2.6 GB |

On Windows, speech recognition runs on the processor, and Parakeet is many times faster than Whisper there.

### What's different on Windows

| | Mac | Windows |
|---|---|---|
| Where the icon lives | Menu bar (top) | System tray (bottom right) |
| System audio permission | Asked once | Not needed |
| Calendar | Reads the Calendar app | Paste your calendar's iCal address |
| Browser call detection | All tabs, after allowing each browser | Tab in front by address, other tabs by name; Chrome and Edge |
| Speech recognition | Uses the graphics chip | Uses the processor (Parakeet recommended) |
| Settings shortcut | Cmd+, | Ctrl+, |

---

## Troubleshooting

Start with **Settings › General › Check setup**. It checks everything a call needs (microphone access, the microphone, call audio, the speech model, the summary engine, the meetings folder and, on Windows with the calendar on, its iCal address) and says what to fix. Click **Test call audio** to make sure Talaan can hear the other people.

### Windows: "Talaan can't use the microphone"

Windows has three microphone switches, and all must be on. Open **Windows Settings › Privacy & security › Microphone** (or click **Open Windows Settings** in Talaan) and turn on:

- **Microphone access**
- **Let apps access your microphone**
- **Let desktop apps access your microphone**

Then click **Check again** in Check setup.

### Windows: Talaan's own sounds, or the call, are recorded twice

Sound "enhancers" and mixers such as **FxSound**, **Voicemeeter** and **SteelSeries Sonar** replay all your computer's sound through their own program. Talaan can then pick up its own sounds as "Them", or capture the call twice. If transcripts show doubled lines or odd echoes, turn the enhancer off during calls, or set your call app's speaker straight to your headphones or speakers.

### Windows: the other people are missing

If "Them" is empty, check the call app's audio settings. When the call app sends sound to a device other than the Windows default (a headset, a virtual device), set its speaker to the same device Windows uses, or to your default device, then run **Test call audio** again.

### Mac: the other people are missing

System audio permission is off. Open **System Settings › Privacy & Security › Screen & System Audio Recording**, find Talaan under the system audio section and turn it on. Then restart Talaan.

### No "Record this call?" when a call starts

- Check **Settings › General › Detect calls** is on.
- Check the app's rule in **Settings › Call apps** isn't **Never**, and that it isn't in **Ignored apps**.
- Browser calls: turn on **Settings › Connections › Browser call detection**. On Windows, if a background call tab isn't recognized, bring it to the front.
- Talaan only notices calls while it's running. Keep **Settings › General › Open at login** on.
- You can always start by hand: click the Talaan icon, then **Start recording**.

### Transcription is slow

Open **Settings › Speech models** and choose **Parakeet**. It is the fastest model, especially on Windows. If you need a language Parakeet doesn't know (Filipino, for example), stay on Whisper and expect it to take longer. A long call keeps working in the background; you can keep using your computer.

### Reporting a problem

Talaan doesn't keep a log file. If something goes wrong, take a screenshot of **Settings › General › Check setup** and open an issue at https://github.com/mmnzns/Talaan/issues with what you were doing.

---

## Removing Talaan

### Step 1, on both: delete your data from inside Talaan

**Settings › Privacy › Delete all Talaan data › Delete everything** removes:

- every meeting and all audio,
- downloaded models,
- your settings,
- saved API keys, the Notion secret and the calendar address,
- the **Open at login** entry.

It can't be undone. It doesn't touch files you exported yourself, pages already sent to Notion, or models Talaan was reusing from another app. Talaan quits by itself when it's done.

If you can't open Talaan any more, skip this and remove the folders by hand as shown below.

### Windows

1. Quit Talaan (tray icon › **Quit**).
2. Open **Windows Settings › Apps › Installed apps**, find **Talaan**, click **...** › **Uninstall**.
3. In the uninstaller, tick **Delete the application data** if you want, then **Uninstall**.

Ticking **Delete the application data** removes everything Talaan stored: your meetings, audio, models and settings (`%APPDATA%\Talaan`), the app's window data, saved API keys, the Notion secret and the calendar address. Leave it unticked to keep your meetings for a later reinstall. The startup entry (**Open at login**) is removed on every uninstall, ticked or not.

Talaan has no setting to keep meetings in another folder: they're always in `%APPDATA%\Talaan`. If you started it with the `TALAAN_DATA_DIR` environment variable (testing only), the uninstaller doesn't know about that folder; delete it by hand. Files you exported, and models Talaan was reusing from another app, are never touched.

If you uninstalled without it and want everything gone, delete these by hand:

| What | Where |
|---|---|
| Program files | `%LOCALAPPDATA%\Talaan` (removed by the uninstaller) |
| Meetings, audio, models, settings | `%APPDATA%\Talaan` |
| App window data | `%LOCALAPPDATA%\com.craftconceptsdigital.talaan` and `%APPDATA%\com.craftconceptsdigital.talaan` |
| Saved API keys | **Control Panel › Credential Manager › Windows Credentials**, under Generic Credentials: `claude.Talaan`, `openai.Talaan`, `gemini.Talaan`, `compatible.Talaan`, `notion.Talaan`, `calendar-ics.Talaan` |

Or paste this into PowerShell:

```powershell
Remove-Item -Recurse -Force "$env:APPDATA\Talaan", "$env:APPDATA\com.craftconceptsdigital.talaan", "$env:LOCALAPPDATA\com.craftconceptsdigital.talaan" -ErrorAction SilentlyContinue
"claude","openai","gemini","compatible","notion","calendar-ics" | ForEach-Object { cmdkey /delete:"$_.Talaan" }
Remove-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Run -Name Talaan -ErrorAction SilentlyContinue
```

(`cmdkey` says "Element not found" for keys you never saved. That's fine.)

### Mac

1. Quit Talaan (menu bar icon › **Quit**).
2. Drag **Talaan** from Applications to the Trash.
3. If you skipped Step 1, remove the rest:

| What | Where |
|---|---|
| Meetings, audio, models, settings | `~/Library/Application Support/Talaan` |
| App data | `~/Library/Application Support/com.craftconceptsdigital.talaan`, `~/Library/Caches/com.craftconceptsdigital.talaan`, `~/Library/WebKit/com.craftconceptsdigital.talaan`, `~/Library/HTTPStorages/com.craftconceptsdigital.talaan`, `~/Library/Preferences/com.craftconceptsdigital.talaan.plist`, `~/Library/Saved Application State/com.craftconceptsdigital.talaan.savedState` |
| Open at login | `~/Library/LaunchAgents/Talaan.plist`, also listed in **System Settings › General › Login Items & Extensions** |
| Saved API keys | **Keychain Access**, search "Talaan": entries named `Talaan` for `claude`, `openai`, `gemini`, `compatible`, `notion` |

Or paste this into Terminal:

```bash
rm -rf "$HOME/Library/Application Support/Talaan" "$HOME/Library/Application Support/com.craftconceptsdigital.talaan" "$HOME/Library/Caches/com.craftconceptsdigital.talaan" "$HOME/Library/WebKit/com.craftconceptsdigital.talaan" "$HOME/Library/HTTPStorages/com.craftconceptsdigital.talaan" "$HOME/Library/Preferences/com.craftconceptsdigital.talaan.plist" "$HOME/Library/Saved Application State/com.craftconceptsdigital.talaan.savedState" "$HOME/Library/LaunchAgents/Talaan.plist"
for a in claude openai gemini compatible notion; do security delete-generic-password -s Talaan -a "$a" >/dev/null 2>&1; done
tccutil reset All com.craftconceptsdigital.talaan
```

The last line clears Talaan from the permission lists in **System Settings › Privacy & Security** (Microphone, Screen & System Audio Recording, Calendars, Automation). You can also remove it from each list by hand with the **-** button.
