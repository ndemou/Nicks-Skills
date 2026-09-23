# Load with: tat completions powershell | Out-String | Invoke-Expression
Register-ArgumentCompleter -Native -CommandName tat,tat.exe -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $elements = @($commandAst.CommandElements)
    $tokens = [System.Collections.Generic.List[string]]::new()
    foreach ($element in $elements) {
        if ($element -is [System.Management.Automation.Language.StringConstantExpressionAst]) {
            $tokens.Add($element.Value)
        } else {
            $tokens.Add($element.Extent.Text)
        }
    }
    if ($elements.Count -gt 0 -and $elements[-1].Extent.EndOffset -eq $cursorPosition) {
        $tokens.RemoveAt($tokens.Count - 1)
    }
    $tokens.Add($wordToComplete)
    $executable = if ($elements[0] -is [System.Management.Automation.Language.StringConstantExpressionAst]) {
        $elements[0].Value
    } else {
        $elements[0].Extent.Text
    }

    $arguments = @('__complete', '--') + $tokens.ToArray()
    & $executable @arguments 2>$null | ForEach-Object {
        $fields = $_ -split "`t", 2
        if ($fields[0]) {
            $description = if ($fields.Count -gt 1) { $fields[1] } else { $fields[0] }
            [System.Management.Automation.CompletionResult]::new(
                $fields[0], $fields[0], 'ParameterValue', $description
            )
        }
    }
}
