//! Simple widget to display an error message.
use yew::{Html, Properties, UseStateHandle, function_component, html};
use yew_icons::{Icon, IconData};

#[derive(Properties, PartialEq)]
pub struct ErrorDisplayProps {
    pub error: UseStateHandle<Option<String>>,
}

#[function_component(ErrorDisplay)]
pub fn error_display_component(props: &ErrorDisplayProps) -> Html {
    let error = (*(props.error.clone())).clone();
    html! {
        if error.is_some() {
            <p class="text-red-800 text-center border border-black m-2 p-2 border-rou rounded-md flex flex-row">
                <Icon data={IconData::LUCIDE_ALERT_TRIANGLE} width={"1rem"} height={"1rem"}/>
                <span class="pl-2">{error.unwrap()}</span>
            </p>
        }
    }
}
