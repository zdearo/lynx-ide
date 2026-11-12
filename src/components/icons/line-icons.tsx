// Icons drawn as lines. Phosphor's fill weight turns these into solid blocks: the close, plus, minus
// and dots look like backgrounds, the carets become big triangles, and the terminal a white window.
import {
  CaretDownIcon as CaretDownFill,
  CaretRightIcon as CaretRightFill,
  CaretUpIcon as CaretUpFill,
  DotsThreeIcon as DotsThreeFill,
  MinusIcon as MinusFill,
  PlusIcon as PlusFill,
  TerminalWindowIcon as TerminalWindowFill,
  XIcon as XFill,
  type Icon,
} from "@phosphor-icons/react";
import { forwardRef } from "react";

function line(Filled: Icon, name: string): Icon {
  const Line = forwardRef<SVGSVGElement, React.ComponentProps<Icon>>(
    (props, ref) => <Filled ref={ref} weight="regular" {...props} />,
  );
  Line.displayName = name;
  return Line;
}

export const CaretDownIcon = line(CaretDownFill, "CaretDownIcon");
export const CaretRightIcon = line(CaretRightFill, "CaretRightIcon");
export const CaretUpIcon = line(CaretUpFill, "CaretUpIcon");
export const DotsThreeIcon = line(DotsThreeFill, "DotsThreeIcon");
export const MinusIcon = line(MinusFill, "MinusIcon");
export const PlusIcon = line(PlusFill, "PlusIcon");
export const TerminalWindowIcon = line(
  TerminalWindowFill,
  "TerminalWindowIcon",
);
export const XIcon = line(XFill, "XIcon");
