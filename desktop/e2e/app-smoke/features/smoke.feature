Feature: Real-app smoke — Rust backend, IPC, and on-disk persistence

  The fake-backend suite proves every UI workflow. This layer proves the things it can't:
  the real built app runs a real scan over a synthetic home, the real Tauri IPC returns it,
  and the real on-disk history store survives a restart. Linux/Windows CI only.

  Scenario: A real scan detects the fixture and opens the workspace
    When I run a scan
    Then the workspace shows an Exposure score
    And at least one finding is listed

  Scenario: Scan history survives an app restart
    When I run a scan
    And I restart the app
    Then the Inbox has at least one run
