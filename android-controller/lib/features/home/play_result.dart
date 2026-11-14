import '../../core/api_client.dart';

/// How "play this playlist / track" ended, for the screen to react to.
enum PlayResult { started, notRunning, notSupported, notFound, failed }

PlayResult playResultFor(Object error) {
  if (error is ApiException) {
    switch (error.code) {
      case ApiErrorCode.appleMusicNotRunning:
        return PlayResult.notRunning;
      case ApiErrorCode.notSupported:
        return PlayResult.notSupported;
      // An unknown playlist id or a song the catalog doesn't know.
      case ApiErrorCode.notFound:
        return PlayResult.notFound;
    }
  }
  return PlayResult.failed;
}

/// A short, plain reason for a failed call, shown under the generic message so
/// a failure on a real phone can be told apart without a debugger.
String failureReason(Object error) {
  if (error is AgentUnreachableException) {
    return switch (error.message) {
      'receiveTimeout' => 'The laptop took too long to answer',
      _ => "The app couldn't reach the laptop",
    };
  }
  if (error is ApiException) {
    final message = error.message.trim();
    return message.isEmpty
        ? 'The laptop answered with an error (${error.status ?? error.code})'
        : message;
  }
  if (error is FormatException) {
    return "The laptop's answer wasn't what the app expected";
  }
  return 'Something unexpected went wrong';
}

/// The snackbar text for a play that did not start, or null if it started.
/// [what] is the singular noun: "playlist" or "track".
String? playFailureMessage(PlayResult result, {required String what}) =>
    switch (result) {
      PlayResult.started => null,
      PlayResult.notRunning => 'Open Apple Music on your laptop',
      PlayResult.notSupported => "Your laptop can't play ${what}s yet",
      PlayResult.notFound => 'That $what is no longer available',
      PlayResult.failed => "Couldn't play that $what",
    };
