import {
  mdiAccountMultiple, mdiAlertOutline, mdiArrowRightBottom, mdiAutoFix, mdiBagPersonal, mdiBookOpenPageVariant, mdiBookSearch,
  mdiCardsOutline, mdiCheck, mdiChevronDown, mdiChevronLeft, mdiChevronRight, mdiChevronUp, mdiClose, mdiCog, mdiContentCopy,
  mdiContentSave, mdiDiceMultiple, mdiDotsHorizontal, mdiDownload, mdiFileDocumentOutline, mdiFilter, mdiFormatListBulletedSquare,
  mdiHammer, mdiInformationOutline, mdiLink, mdiLock, mdiMagnify, mdiMenu, mdiMinus, mdiOpenInApp, mdiPackageVariantClosed,
  mdiPencil, mdiPlus, mdiRobotOutline, mdiScriptText, mdiSend, mdiShield, mdiStar, mdiStop, mdiSwordCross, mdiTrashCanOutline,
  mdiUpload, mdiWeb, mdiCheckboxMarkedOutline, mdiDotsVertical, mdiTagMultiple, mdiImageMultipleOutline, mdiMicrophone,
  mdiFolderSearchOutline, mdiFolderRemoveOutline, mdiBackupRestore,
} from "@mdi/js";

/** Icon set: Material Design Icons (Apache-2.0) via @mdi/js. */
const PATHS = {
  users: mdiAccountMultiple,
  hammer: mdiHammer,
  sparkles: mdiAutoFix,
  backpack: mdiBagPersonal,
  scroll: mdiScriptText,
  book: mdiBookOpenPageVariant,
  box: mdiPackageVariantClosed,
  bot: mdiRobotOutline,
  cog: mdiCog,
  pen: mdiPencil,
  search: mdiMagnify,
  plus: mdiPlus,
  minus: mdiMinus,
  x: mdiClose,
  check: mdiCheck,
  chevron: mdiChevronRight,
  back: mdiChevronLeft,
  more: mdiDotsHorizontal,
  "more-vertical": mdiDotsVertical,
  dice: mdiDiceMultiple,
  save: mdiContentSave,
  star: mdiStar,
  trash: mdiTrashCanOutline,
  copy: mdiContentCopy,
  upload: mdiUpload,
  download: mdiDownload,
  up: mdiChevronUp,
  down: mdiChevronDown,
  shield: mdiShield,
  link: mdiLink,
  send: mdiSend,
  stop: mdiStop,
  lock: mdiLock,
  filter: mdiFilter,
  menu: mdiMenu,
  file: mdiFileDocumentOutline,
  swords: mdiSwordCross,
  cards: mdiCardsOutline,
  "next-selection": mdiArrowRightBottom,
  "selection-flow": mdiFormatListBulletedSquare,
  globe: mdiWeb,
  "book-search": mdiBookSearch,
  "open-in-app": mdiOpenInApp,
  info: mdiInformationOutline,
  alert: mdiAlertOutline,
  "check-box": mdiCheckboxMarkedOutline,
  tags: mdiTagMultiple,
  image: mdiImageMultipleOutline,
  mic: mdiMicrophone,
  "folder-search": mdiFolderSearchOutline,
  "folder-remove": mdiFolderRemoveOutline,
  restore: mdiBackupRestore,
} as const;

export type IconName = keyof typeof PATHS;

export function Icon({ name, label }: { name: IconName; label?: string }) {
  return (
    <svg className="icon" viewBox="0 0 24 24" role={label ? "img" : undefined} aria-label={label} aria-hidden={label ? undefined : true} focusable="false">
      <path d={PATHS[name]} />
    </svg>
  );
}
