//
//  MCPSettingsPanel.swift
//  UnpeelNative
//
//  Settings ▸ Unpeel MCP ▸ MCP Settings: the group's first page.
//

import SwiftUI
import UnpeelShared

/// What the unpeel MCP server is and which agent CLIs on the selected Host
/// are connected to it (Unpeel's lifecycle hooks + the server registered in
/// the CLI's own configuration). Connecting runs the Host's
/// `integrations.install` verb; the per-agent copy comes from the runtime
/// package, so the Controller knows nothing about provider file formats and
/// a remote Host renders identically. The domain policies (Sessions use,
/// Browser use) are the sibling pages.
struct MCPSettingsPanel: View {
    @ObservedObject var store: UnpeelStore
    @ObservedObject var runtime: RemoteHostRuntime
    @State private var pendingIntegrations: Set<String> = []
    @State private var integrationError: String?
    @State private var showManualSetup = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Form {
                Section {} header: {
                    SettingsPaneHeader(
                        title: "Unpeel MCP",
                        description: "Every capable agent gets the unpeel MCP server: your "
                            + "other sessions, a real browser, artifacts, and Apps. Connect an "
                            + "agent once per Host; Unpeel keeps it current after upgrades. "
                            + "What each domain may do is set on the pages that follow."
                    )
                    .padding(.bottom, 4)
                }

                connectedAgentsSection
            }
            .formStyle(.grouped)
            .scrollContentBackground(.hidden)
        }
    }

    // MARK: Connected agents

    private var connectableAgents: [RemoteAgentSummary] {
        (runtime.snapshot?.workspaceSettings?.availableAgents ?? []).filter { $0.installed }
    }
    private var canConnect: Bool {
        runtime.supportsHostOperation(RemoteHostRuntime.HostOperation.integrationsInstall)
    }

    /// One row per agent CLI present on the Host: connected (hooks + MCP
    /// registered in that CLI's own config), connectable, or detection-only.
    /// Connecting runs the Host's `integrations.install` verb; the row copy
    /// comes from the runtime package, so the Controller knows nothing about
    /// provider file formats.
    private var connectedAgentsSection: some View {
        Section {
            if connectableAgents.isEmpty {
                Text("No agent CLIs were found on this Host's PATH. Install one under "
                    + "Agents & Apps and it will appear here.")
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.mutedForeground)
            }
            ForEach(connectableAgents) { agent in
                connectedAgentRow(agent)
            }
            if let integrationError {
                Text(integrationError)
                    .font(.system(size: 11))
                    .foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
            }
            manualSetup
        } header: {
            SettingsSectionHeader(
                title: "Connected agents",
                description: "An unconnected agent still shows busy and idle from its screen. "
                    + "Connecting registers Unpeel's hooks and the unpeel MCP server in the "
                    + "agent's own configuration, for exact status, notifications, precise "
                    + "resume, and Unpeel's tools inside the agent."
            )
        }
    }

    private func connectedAgentRow(_ agent: RemoteAgentSummary) -> some View {
        let connected = agent.integrationInstalled ?? false
        let installable = agent.integrationInstallable ?? false
        return LabeledContent {
            if !installable {
                Text("Detection only")
                    .font(.system(size: 11))
                    .foregroundStyle(Theme.mutedForeground)
            } else if pendingIntegrations.contains(agent.id) {
                ProgressView().controlSize(.small)
            } else if connected {
                HStack(spacing: 8) {
                    Label("Connected", systemImage: "checkmark.circle.fill")
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.mutedForeground)
                    Button("Reinstall") { connect(agent) }
                        .buttonStyle(.plain)
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.mutedForeground)
                        .disabled(!canConnect)
                        .help("Rewrite the hook script and MCP registration for this Host build")
                }
            } else {
                Button("Connect") { connect(agent) }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .disabled(!canConnect)
                    .help(canConnect
                        ? "Register Unpeel's hooks and MCP server with \(agent.name)"
                        : "This Host does not support connecting agents")
            }
        } label: {
            HStack(alignment: .top, spacing: 10) {
                ToolIconView(appID: nil, command: agent.command, size: 18)
                    .frame(width: 22, height: 22)
                VStack(alignment: .leading, spacing: 2) {
                    Text(agent.name)
                        .font(.system(size: 13))
                        .foregroundStyle(Theme.foreground)
                    Text(installable
                        ? (agent.integrationSummary ?? "Registers Unpeel's hooks and MCP server in the agent's own configuration.")
                        : "This agent has no hooks or MCP configuration Unpeel can register; it keeps identity from detection and resume from its own continue-last.")
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.mutedForeground)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }

    private func connect(_ agent: RemoteAgentSummary) {
        pendingIntegrations.insert(agent.id)
        integrationError = nil
        Task { @MainActor in
            defer { pendingIntegrations.remove(agent.id) }
            do { try await runtime.installIntegration(runtimeID: agent.id) }
            catch { integrationError = "\(agent.name): \(error.localizedDescription)" }
        }
    }

    /// The transparent alternative: the shim path and each provider's own
    /// command. Registering by hand gives the MCP half only; hooks still
    /// need Connect.
    @ViewBuilder
    private var manualSetup: some View {
        let shim = runtime.snapshot?.workspaceSettings?.mcpShimPath
        let manual = connectableAgents.compactMap { agent -> (String, String)? in
            guard let command = agent.integrationManualCommand else { return nil }
            return (agent.name, command)
        }
        DisclosureGroup("Manual setup", isExpanded: $showManualSetup) {
            VStack(alignment: .leading, spacing: 6) {
                Text("Every provider's MCP config points at one launcher on this Host. "
                    + "Register it with the agent's own command to get Unpeel's tools "
                    + "without the hooks; \"Connect\" does both.")
                    .font(.system(size: 11))
                    .foregroundStyle(Theme.mutedForeground)
                    .fixedSize(horizontal: false, vertical: true)
                if let shim {
                    Text(shim)
                        .font(.system(size: 11, design: .monospaced))
                        .textSelection(.enabled)
                }
                ForEach(manual, id: \.0) { name, command in
                    VStack(alignment: .leading, spacing: 1) {
                        Text(name).font(.system(size: 11)).foregroundStyle(Theme.mutedForeground)
                        Text(command)
                            .font(.system(size: 11, design: .monospaced))
                            .textSelection(.enabled)
                    }
                }
                Text("Or from any terminal: unpeel integrations install <agent>")
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(Theme.mutedForeground)
            }
            .padding(.top, 4)
        }
        .font(.system(size: 12))
    }

}
