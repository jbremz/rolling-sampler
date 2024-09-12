# Rolling Sampler
## Overview
This project is a desktop application for recording and visualising real-time audio input. The app allows users to record audio from their microphone or other input devices, save the recorded audio as a .wav file, and view a rolling waveform of the audio input in real-time. The user can also select the folder where recordings will be saved, choose the input device, and adjust the buffer size to control how much past audio is included in the saved recording.

## Features
- Real-time Audio Visualisation: Displays a rolling waveform of audio input.
- Device Selection: Allows selection of available input audio devices.
- Adjustable Buffer Size: Users can choose how much past audio to include in the recording (in seconds).
- Save Recordings: Saves the recorded audio as a .wav file, including the option to select the save folder.
- Cross-platform: Built with cpal, eframe, and egui, making it compatible with multiple platforms (Linux, macOS, Windows).

## Prerequisites
Before running this application, ensure you have the following installed:
- Rust (latest stable version)
- cpal for audio input/output.
- eframe and egui for the graphical user interface.
- hound for writing .wav files.
- rfd for native file dialogs.
- chrono for timestamps in file naming.

## Usage
1. Clone the repository:
```bash
git clone https://github.com/your-username/rolling-sampler.git
cd rolling-sampler
```
2. Build the project:
```bash
cargo build --release
```
3. Run the project:
```bash
cargo run
```
This will launch the GUI, where you can start interacting with the application.

## How to Use
1. Select Input Device: Use the dropdown menu to select your desired input device (e.g., microphone).
2. Adjust Buffer Size: The slider allows you to change how much past audio is stored before saving (in seconds).
3. Start/Stop Recording: Click "Start Grab" to begin capturing audio. Click "Stop Grab" to stop and save the recording.
4. Select Save Folder: You can choose where the .wav files will be saved using the "Select Save Folder" button.
5. View Waveform: A rolling waveform of the incoming audio is displayed in the UI.

## Code Structure
- main.rs: Contains the core application logic, including real-time audio recording, waveform visualisation, and UI components.
- Recorder: Manages audio input, buffer handling, and .wav file writing.
- CircularBuffer: Circular buffer to store and manage audio samples, allowing both real-time visualisation and static mode for finalising recordings.

## Dependencies
The project relies on the following crates:
- cpal: For interacting with audio devices.
- eframe and egui: For building the graphical user interface.
- hound: To save recordings as .wav files.
- rfd: To open native file dialogs.
- chrono: For timestamp-based file names.
- dirs: For determining the default save path (Desktop).

## Future Improvements
- Add more customisation options for audio format (bit depth, sample rate, channels)
- Implement zooming and panning for waveform visualisation
- Add more advanced recording features (e.g., pausing, resuming)

## License
This project is licensed under the MIT License - see the LICENSE file for details.

Feel free to contribute by submitting issues or pull requests!