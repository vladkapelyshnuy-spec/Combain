import { useEffect, useState } from "react";

export type AppPath = "/" | "/ui-states";

const resolvePath = (pathname: string): AppPath =>
  pathname === "/ui-states" ? "/ui-states" : "/";

export function useAppRouter() {
  const [currentPath, setCurrentPath] = useState<AppPath>(() =>
    resolvePath(window.location.pathname),
  );

  useEffect(() => {
    const handlePopState = () => setCurrentPath(resolvePath(window.location.pathname));
    window.addEventListener("popstate", handlePopState);
    return () => window.removeEventListener("popstate", handlePopState);
  }, []);

  const navigate = (path: AppPath) => {
    if (path === currentPath) return;
    window.history.pushState({}, "", path);
    setCurrentPath(path);
  };

  return { currentPath, navigate };
}
