;;; alan-ui.el --- Quiet UI defaults for alan-emacs -*- lexical-binding: t; -*-

;;; Commentary:
;; Light, native-feeling defaults without third-party theme packages.

;;; Code:

(when (fboundp 'tool-bar-mode)
  (tool-bar-mode -1))
(when (fboundp 'menu-bar-mode)
  (menu-bar-mode -1))
(when (fboundp 'scroll-bar-mode)
  (scroll-bar-mode -1))

(blink-cursor-mode -1)
(column-number-mode 1)
(global-hl-line-mode 1)
(global-display-line-numbers-mode 1)

(setq display-line-numbers-type 'relative)
(setq visible-bell nil)
(setq frame-title-format '("%b - alan-emacs"))

(when (member 'modus-operandi (custom-available-themes))
  (load-theme 'modus-operandi t))

(when (display-graphic-p)
  (set-face-attribute 'default nil :height 140))

(provide 'alan-ui)
;;; alan-ui.el ends here
