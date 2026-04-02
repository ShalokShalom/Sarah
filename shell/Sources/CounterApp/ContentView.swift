// ContentView.swift — SwiftUI shell view.
//
// SHELL LAYER — pure UI. No business logic lives here.
//
// This view renders the state published by CounterViewModel and dispatches
// user actions back to it. The ViewModel in turn calls the Rust Core.
//
// Data flow (Unidirectional):
//
//   User Tap  ──▶  ViewModel.action()  ──▶  Rust Core  ──▶  @Published state  ──▶  View re-render

import SwiftUI

struct ContentView: View {
    @StateObject private var viewModel = CounterViewModel()

    var body: some View {
        NavigationStack {
            VStack(spacing: 32) {
                counterDisplay
                controlButtons
                errorBanner
            }
            .padding(24)
            .navigationTitle("Counter")
            .navigationBarTitleDisplayMode(.inline)
        }
    }

    // MARK: — Sub-views

    private var counterDisplay: some View {
        VStack(spacing: 8) {
            Text(viewModel.displayValue)
                .font(.system(size: 72, weight: .bold, design: .rounded))
                .monospacedDigit()
                .foregroundStyle(viewModel.hasError ? .red : .primary)
                .contentTransition(.numericText())
                .animation(.spring(response: 0.3), value: viewModel.displayValue)

            Text("Current value")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 24)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 20))
    }

    private var controlButtons: some View {
        VStack(spacing: 16) {
            HStack(spacing: 24) {
                CounterButton(
                    label: "−",
                    color: .red,
                    action: viewModel.decrement
                )

                CounterButton(
                    label: "+",
                    color: .green,
                    action: viewModel.increment
                )
            }

            HStack(spacing: 16) {
                CounterButton(
                    label: "+10",
                    color: .blue,
                    compact: true,
                    action: { viewModel.add(10) }
                )

                CounterButton(
                    label: "−10",
                    color: .orange,
                    compact: true,
                    action: { viewModel.add(-10) }
                )

                CounterButton(
                    label: "Reset",
                    color: .gray,
                    compact: true,
                    action: viewModel.reset
                )
            }
        }
    }

    @ViewBuilder
    private var errorBanner: some View {
        if let message = viewModel.errorMessage {
            HStack(spacing: 8) {
                Image(systemName: "exclamationmark.triangle.fill")
                Text(message)
                    .font(.subheadline)
            }
            .foregroundStyle(.white)
            .padding()
            .frame(maxWidth: .infinity)
            .background(.red.gradient, in: RoundedRectangle(cornerRadius: 12))
            .transition(.move(edge: .bottom).combined(with: .opacity))
        }
    }
}

// MARK: — Reusable Button Component

struct CounterButton: View {
    let label: String
    let color: Color
    var compact: Bool = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(label)
                .font(compact
                      ? .system(.callout, design: .rounded, weight: .semibold)
                      : .system(size: 36, weight: .bold, design: .rounded))
                .frame(
                    width:  compact ? nil   : 80,
                    height: compact ? nil   : 80
                )
                .padding(compact ? .horizontal : [])
                .padding(compact ? 10          : 0)
                .foregroundStyle(.white)
                .background(color.gradient, in: RoundedRectangle(cornerRadius: compact ? 12 : 40))
        }
        .buttonStyle(.plain)
    }
}

// MARK: — Preview

#Preview {
    ContentView()
}
