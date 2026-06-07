;;; alan-git.el --- Built-in Git workflow for alan-emacs -*- lexical-binding: t; -*-

;;; Commentary:
;; Start with Emacs VC. Third-party Git UI packages can be added after the
;; vanilla baseline is stable.

;;; Code:

(require 'vc)

(global-set-key (kbd "C-c g s") #'vc-dir)
(global-set-key (kbd "C-c g l") #'vc-print-root-log)
(global-set-key (kbd "C-c g b") #'vc-annotate)
(global-set-key (kbd "C-c g d") #'vc-root-diff)

(provide 'alan-git)
;;; alan-git.el ends here
