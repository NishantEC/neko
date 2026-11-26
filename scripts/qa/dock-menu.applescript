-- Reads the Dock menu of one app once: osascript scripts/qa/dock-menu.applescript "NAME"
-- Opens the Dock item's menu (AXShowMenu), prints its item names one per line, then closes it with Escape. The menu shows for
-- about a second. Used by the QA rig's --window mode (docs/knowledge/qa.md); needs Accessibility access for the terminal.
on run argv
	set itemName to item 1 of argv
	tell application "System Events"
		tell process "Dock"
			considering case
				set found to missing value
				repeat with e in (UI elements of list 1)
					if name of e is itemName then
						set found to e
						exit repeat
					end if
				end repeat
			end considering
			if found is missing value then return "NO DOCK ITEM"
			perform action "AXShowMenu" of found
			delay 0.8
			set names to {}
			try
				set names to name of every menu item of menu 1 of found
			end try
			key code 53
		end tell
	end tell
	set out to ""
	repeat with n in names
		if n is not missing value then set out to out & n & linefeed
	end repeat
	return out
end run
