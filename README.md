# Buffer sample

### **Project Overview**

Develop a standalone rolling buffer recording application for macOS that streams audio directly into RAM and holds a certain period of audio (e.g., up to 10 minutes). The user can save selected segments of audio by pressing a button, which allows saving audio from a specified time before the button was pressed. The application will include a GUI with a waveform display, configurable input settings, and a directory management system for saved files.

### **Core Features**

1. **Audio Streaming & Buffering:**
    - Stream audio directly into RAM.
    - Maintain a rolling buffer to store up to 10 minutes of audio.
    - Enable user-configurable start positions for saving audio (e.g., -0s, -5s, -10s, -30s, -1min).
2. **Audio Saving:**
    - Provide buttons to select and save audio from specific start positions.
    - Include an option to extend the saved portion slightly further back if the user didn’t capture the entire desired segment.
    - Implement a file-naming convention for saved audio files.
3. **Waveform Display:**
    - Display a real-time waveform of the buffered audio.
    - Allow the user to visually track the audio and identify segments of interest.
4. **GUI for User Interaction:**
    - A user-friendly interface to interact with the application.
    - Input configuration settings (e.g., audio input source, buffer length).
    - Directory selection for saving audio files.
5. **Settings Management:**
    - Allow the user to configure input settings such as buffer duration, input device selection, and save directory.
    - Provide a settings section within the GUI for easy adjustments.