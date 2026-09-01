export const appRoutes = [
  {
    path: "/projects",
    label: "Projects",
    description: "Create and manage your video projects.",
    icon: "projects",
  },
  {
    path: "/script",
    label: "Script",
    description: "Shape the topic, narrative, and voice of your video.",
    icon: "script",
  },
  {
    path: "/scenes",
    label: "Scenes",
    description: "Organize the building blocks of your story.",
    icon: "scenes",
  },
  {
    path: "/storyboard",
    label: "Storyboard",
    description: "Review the visual direction scene by scene.",
    icon: "storyboard",
  },
  {
    path: "/media",
    label: "Media",
    description: "Keep generated images, video, and audio organized.",
    icon: "media",
  },
  {
    path: "/timeline",
    label: "Timeline",
    description: "Arrange selected assets into the final sequence.",
    icon: "timeline",
  },
  {
    path: "/export",
    label: "Export",
    description: "Prepare the completed project for delivery.",
    icon: "export",
  },
  {
    path: "/settings",
    label: "Settings",
    description: "Configure application and provider preferences.",
    icon: "settings",
  },
] as const;

export type AppRoute = (typeof appRoutes)[number];
export type AppPath = AppRoute["path"];
export type SectionIconName = AppRoute["icon"];

export const defaultPath: AppPath = "/projects";

export function findRoute(pathname: string): AppRoute {
  return appRoutes.find((route) => route.path === pathname) ?? appRoutes[0];
}
