import type { SVGProps } from "react";
import type { SectionIconName } from "../routing/routes";

type SectionIconProps = SVGProps<SVGSVGElement> & {
  name: SectionIconName;
};

export function SectionIcon({ name, ...props }: SectionIconProps) {
  const commonProps = {
    ...props,
    "aria-hidden": true,
    fill: "none",
    viewBox: "0 0 24 24",
    xmlns: "http://www.w3.org/2000/svg",
  };

  switch (name) {
    case "projects":
      return <svg {...commonProps}><path d="M3.75 6.75h6l1.5 2.25h9v9.75a1.5 1.5 0 0 1-1.5 1.5h-15v-13.5Z" /><path d="M3.75 6.75v-1.5a1.5 1.5 0 0 1 1.5-1.5h4.5l1.5 2.25h7.5a1.5 1.5 0 0 1 1.5 1.5V9" /></svg>;
    case "script":
      return <svg {...commonProps}><path d="M6 3.75h8.25L18 7.5v12.75H6V3.75Z" /><path d="M14.25 3.75V7.5H18M9 12h6M9 15.75h4.5" /></svg>;
    case "scenes":
      return <svg {...commonProps}><rect x="3.75" y="5.25" width="16.5" height="13.5" rx="1.5" /><path d="m8.25 5.25 2.25 3M13.5 5.25l2.25 3M3.75 9h16.5" /></svg>;
    case "storyboard":
      return <svg {...commonProps}><rect x="3.75" y="3.75" width="6.75" height="6.75" rx="1" /><rect x="13.5" y="3.75" width="6.75" height="6.75" rx="1" /><rect x="3.75" y="13.5" width="6.75" height="6.75" rx="1" /><rect x="13.5" y="13.5" width="6.75" height="6.75" rx="1" /></svg>;
    case "media":
      return <svg {...commonProps}><rect x="3.75" y="4.5" width="16.5" height="15" rx="1.5" /><circle cx="8.25" cy="9" r="1.5" /><path d="m5.25 17.25 4.5-4.5 3 3 2.25-2.25 3.75 3.75" /></svg>;
    case "timeline":
      return <svg {...commonProps}><path d="M3.75 6.75h16.5M3.75 17.25h16.5M7.5 3.75v6M15 14.25v6" /><rect x="7.5" y="4.5" width="7.5" height="4.5" rx="1" /><rect x="7.5" y="15" width="7.5" height="4.5" rx="1" /></svg>;
    case "export":
      return <svg {...commonProps}><path d="M12 15V3.75M7.5 8.25 12 3.75l4.5 4.5M4.5 13.5v6.75h15V13.5" /></svg>;
  }
}
