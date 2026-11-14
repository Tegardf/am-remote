package dev.amremote.am_remote

import com.ryanheise.audioservice.AudioServiceActivity

// audio_service needs its own activity so the media notification can reach the Flutter engine.
class MainActivity : AudioServiceActivity()
