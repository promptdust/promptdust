Feature: Exporting a report

  Scenario: Export writes a markdown report and confirms where it went
    Given a machine with AI-tool data present
    And no previous scans
    When I open PromptDust
    And I click "Scan"
    And I open the list menu
    And I click "Export report…"
    Then a toast says "Saved to"
