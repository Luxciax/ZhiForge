import React from 'react'
import ReactDOM from 'react-dom/client'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { ResultWindow } from './windows/result/ResultWindow'
import { Toolbar } from './windows/toolbar/Toolbar'
import './styles.css'
import './ui-final-polish.css'

const currentWindow = getCurrentWindow()

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    {currentWindow.label === 'result' ? <ResultWindow /> : <Toolbar />}
  </React.StrictMode>
)
