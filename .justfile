# run check
check:
	x check
alias chpm := check_pm
# run check for proc_macro
check_pm:
	x check proc_macro

build_pm:
    x build --stage 1 proc_macro

alias cre := check_rustc_expand
# run check for rustc_expand
check_rustc_expand:
	x check rustc_expand

# ./x build --stage 1 rustc_metadata
brm:
	./x build --stage 1 rustc_metadata	

# git push origin HEAD:wpm --no-verify
push:
	git push origin HEAD:wpm --no-verify 
