import { useCallback, useEffect, useState } from "react"

type Theme = "light" | "dark" | "system"

function systemDark() {
  return window.matchMedia("(prefers-color-scheme: dark)").matches
}

function apply(theme: Theme) {
  const dark = theme === "dark" || (theme === "system" && systemDark())
  document.documentElement.classList.toggle("dark", dark)
  return dark ? "dark" : "light"
}

export function useTheme() {
  const [theme, setTheme] = useState<Theme>(() => {
    try {
      return (localStorage.getItem("cmw-theme") as Theme) || "system"
    } catch {
      return "system"
    }
  })
  const [resolved, setResolved] = useState<"light" | "dark">(() => (document.documentElement.classList.contains("dark") ? "dark" : "light"))

  useEffect(() => {
    setResolved(apply(theme))
    try {
      localStorage.setItem("cmw-theme", theme)
    } catch {
      /* stockage indisponible */
    }
    if (theme !== "system") return
    const mq = window.matchMedia("(prefers-color-scheme: dark)")
    const onChange = () => setResolved(apply("system"))
    mq.addEventListener("change", onChange)
    return () => mq.removeEventListener("change", onChange)
  }, [theme])

  const toggle = useCallback(() => setTheme(resolved === "dark" ? "light" : "dark"), [resolved])
  return { theme, resolved, setTheme, toggle }
}
