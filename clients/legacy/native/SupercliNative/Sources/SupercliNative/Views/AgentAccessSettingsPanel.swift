//
//  AgentAccessSettingsPanel.swift
//  SupercliNative
//
//  Settings ▸ Agent access: what agents may do through Supercli's tools.
//

import SwiftUI

/// One page for the unified supercli MCP server's policies: the Sessions
/// domain (open reads, approval-controlled writes, worktrees, gallery,
/// remembered pairs) and the Browser domain (engine, access default,
/// approvals, options, site rules). Which agents *have* the server is the
/// Agents page; this page is what they may do with it. Each half follows
/// its feature gate (`sessionsMcp`, `browserMcp`).
struct AgentAccessSettingsPanel: View {
    @ObservedObject var store: SupercliStore

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Form {
                Section {} header: {
                    SettingsPaneHeader(
                        title: "Agent access",
                        description: "What connected agents may do through Supercli's tools: read and "
                            + "type into your other sessions, drive a real browser, prepare "
                            + "worktrees, and publish to the gallery. Reads are open; every write "
                            + "follows the policies here. This is a cooperative policy for agents "
                            + "using Supercli's tools, not a sandbox."
                    )
                    .padding(.bottom, 4)
                }

                if SupercliFeatureFlags.isEnabled(.sessionsMcp) {
                    SessionsAccessSections(store: store)
                }
                if SupercliFeatureFlags.isEnabled(.browserMcp) {
                    BrowserAccessSections(store: store)
                }
                if !SupercliFeatureFlags.isEnabled(.sessionsMcp), !SupercliFeatureFlags.isEnabled(.browserMcp) {
                    Section {
                        Text("Turn on Sessions use or Browser use under Features to configure agent access.")
                            .font(.system(size: 13))
                            .foregroundStyle(Theme.mutedForeground)
                    }
                }
            }
            .formStyle(.grouped)
            .scrollContentBackground(.hidden)
        }
    }
}
