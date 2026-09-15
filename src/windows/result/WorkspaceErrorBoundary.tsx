import { Component, type ErrorInfo, type ReactNode } from 'react'

type WorkspaceErrorBoundaryProps = {
  children: ReactNode
  resetToken?: unknown
}

// Keep ResultWindow and its action listeners mounted if a workspace view fails.
export class WorkspaceErrorBoundary extends Component<
  WorkspaceErrorBoundaryProps,
  { failed: boolean }
> {
  state = { failed: false }

  static getDerivedStateFromError() {
    return { failed: true }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('Workspace rendering failed', error, info.componentStack)
  }

  componentDidUpdate(previousProps: Readonly<WorkspaceErrorBoundaryProps>) {
    if (this.state.failed && previousProps.resetToken !== this.props.resetToken) {
      this.setState({ failed: false })
    }
  }

  render() {
    if (this.state.failed) {
      return (
        <section className="result-empty" role="alert">
          <span>当前页面显示失败，请重试。划词工具仍可使用。</span>
          <button type="button" onClick={() => this.setState({ failed: false })}>重新打开</button>
        </section>
      )
    }
    return this.props.children
  }
}
