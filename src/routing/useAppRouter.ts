import { useEffect, useState } from "react";
import { defaultPath, findRoute, type AppPath } from "./routes";

const resolvePath = (pathname: string): AppPath => findRoute(pathname).path;

export function useAppRouter() {
  const [currentPath, setCurrentPath] = useState<AppPath>(() =>
    resolvePath(window.location.pathname),
  );

  useEffect(() => {
    if (window.location.pathname !== currentPath) {
      window.history.replaceState({}, "", defaultPath);
    }

    const handlePopState = () => setCurrentPath(resolvePath(window.location.pathname));
    window.addEventListener("popstate", handlePopState);
    return () => window.removeEventListener("popstate", handlePopState);
  }, [currentPath]);

  const navigate = (path: AppPath) => {
    if (path === currentPath) return;
    window.history.pushState({}, "", path);
    setCurrentPath(path);
  };

  return { currentPath, navigate };
}
